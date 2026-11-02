use crate::{
    manifest::{safe_path, FileKind, Linkage, PackageFile, MANIFEST_LIMIT},
    Error, Manifest, Result,
};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{BufRead, BufReader, Cursor, Read},
};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub compressed_bytes: u64,
    pub expanded_bytes: u64,
    pub file_bytes: u64,
    pub entries: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            compressed_bytes: 64 * 1024 * 1024,
            expanded_bytes: 256 * 1024 * 1024,
            file_bytes: 64 * 1024 * 1024,
            entries: 4096,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct Inspection {
    pub manifest: Manifest,
    pub files: BTreeMap<String, FileInfo>,
    pub authenticated: bool,
    pub warning: &'static str,
}
struct Bounded<R> {
    inner: R,
    remaining: u64,
    label: &'static str,
}
impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let allowed = out
            .len()
            .min(self.remaining.saturating_add(1).min(usize::MAX as u64) as usize);
        let n = self.inner.read(&mut out[..allowed])?;
        if n as u64 > self.remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ZPK {} size limit exceeded", self.label),
            ));
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}
fn bad(message: impl Into<String>) -> Error {
    Error::Archive(message.into())
}
fn octal(bytes: &[u8]) -> Result<u64> {
    let trimmed = bytes
        .iter()
        .copied()
        .skip_while(|b| *b == b' ' || *b == 0)
        .collect::<Vec<_>>();
    let digits = trimmed
        .split(|b| *b == 0 || *b == b' ')
        .next()
        .unwrap_or_default();
    if trimmed[digits.len()..]
        .iter()
        .any(|b| *b != 0 && *b != b' ')
        || digits.iter().any(|b| !(b'0'..=b'7').contains(b))
    {
        return Err(bad("non-octal tar number (base-256 is unsupported)"));
    }
    if digits.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(
        std::str::from_utf8(digits).map_err(|_| bad("non-ASCII tar number"))?,
        8,
    )
    .map_err(|_| bad("tar number overflow"))
}
fn string(bytes: &[u8]) -> Result<&str> {
    let n = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if bytes[n..].iter().any(|b| *b != 0) {
        return Err(bad("hidden bytes after tar string terminator"));
    }
    std::str::from_utf8(&bytes[..n]).map_err(|_| bad("non-UTF8 tar path"))
}
struct Header {
    path: String,
    size: u64,
    mode: u64,
    directory: bool,
}
fn header(block: &[u8; 512]) -> Result<Header> {
    let sum: u64 = block
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if (148..156).contains(&i) {
                32
            } else {
                u64::from(*b)
            }
        })
        .sum();
    if octal(&block[148..156])? != sum {
        return Err(bad("tar header checksum mismatch"));
    }
    if &block[257..263] != b"ustar\0"
        || &block[263..265] != b"00"
        || block[500..].iter().any(|b| *b != 0)
    {
        return Err(bad("only POSIX USTAR headers are supported"));
    }
    let directory = match block[156] {
        0 | b'0' => false,
        b'5' => true,
        _ => {
            return Err(bad(
                "links, devices, sparse files and tar extensions are forbidden",
            ))
        }
    };
    if !string(&block[157..257])?.is_empty()
        || octal(&block[108..116])? != 0
        || octal(&block[116..124])? != 0
        || octal(&block[329..337])? != 0
        || octal(&block[337..345])? != 0
    {
        return Err(bad(
            "link targets, nonzero ownership or device numbers are forbidden",
        ));
    }
    octal(&block[136..148])?;
    let name = string(&block[..100])?;
    let prefix = string(&block[345..500])?;
    let raw = if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    };
    let path = if directory {
        raw.strip_suffix('/').unwrap_or(&raw)
    } else {
        &raw
    };
    if !safe_path(path) {
        return Err(bad(format!("unsafe/noncanonical archive path: {path}")));
    }
    let size = octal(&block[124..136])?;
    let mode = octal(&block[100..108])?;
    if (directory && (size != 0 || mode != 0o755))
        || (!directory && ![0o644, 0o755].contains(&mode))
    {
        return Err(bad(
            "invalid tar mode/size; no setuid/setgid/writable metadata",
        ));
    }
    Ok(Header {
        path: path.to_owned(),
        size,
        mode,
        directory,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checksums {
    schema_version: u32,
    algorithm: String,
    #[serde(deserialize_with = "unique_hashes")]
    files: BTreeMap<String, String>,
}
fn unique_hashes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    struct Unique;
    impl<'de> Visitor<'de> for Unique {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("unique canonical path -> lowercase SHA-256 map")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut out = BTreeMap::new();
            while let Some((path, hash)) = map.next_entry::<String, String>()? {
                if !safe_path(&path)
                    || hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || out.insert(path, hash).is_some()
                {
                    return Err(serde::de::Error::custom(
                        "invalid or duplicate checksum path/hash",
                    ));
                }
            }
            Ok(out)
        }
    }
    deserializer.deserialize_map(Unique)
}

/// Inspect one zstd frame containing strict USTAR. No extraction, subprocesses or package execution.
/// The caller must provide a bounded/local source: `Read` itself cannot impose a wall-clock deadline.
pub fn inspect<R: Read>(input: R, limits: Limits) -> Result<Inspection> {
    if limits.entries == 0
        || limits.compressed_bytes == 0
        || limits.expanded_bytes == 0
        || limits.file_bytes == 0
    {
        return Err(Error::Limit("all limits must be positive".into()));
    }
    let mut compressed = Bounded {
        inner: input,
        remaining: limits.compressed_bytes,
        label: "compressed",
    };
    let mut magic = [0u8; 4];
    compressed.read_exact(&mut magic)?;
    if magic != [0x28, 0xb5, 0x2f, 0xfd] {
        return Err(bad("expected a standard zstd frame"));
    }
    let buffered = BufReader::new(Cursor::new(magic).chain(compressed));
    let mut decoder = zstd::stream::read::Decoder::with_buffer(buffered)?.single_frame();
    decoder.window_log_max(23)?; // At most 8 MiB decoder window, independently of expansion limits.
    let mut expanded = Bounded {
        inner: decoder,
        remaining: limits.expanded_bytes,
        label: "expanded",
    };
    let result = read_tar(&mut expanded, limits)?;
    let mut buffered = expanded.inner.finish();
    if !buffered.fill_buf()?.is_empty() {
        return Err(bad("trailing data or concatenated zstd frame"));
    }
    Ok(result)
}
fn read_tar(input: &mut impl Read, limits: Limits) -> Result<Inspection> {
    let mut seen = BTreeSet::new();
    let mut manifest: Option<Manifest> = None;
    let mut files = BTreeMap::new();
    let mut checksums: Option<Checksums> = None;
    loop {
        let mut block = [0u8; 512];
        input.read_exact(&mut block)?;
        if block.iter().all(|b| *b == 0) {
            input.read_exact(&mut block)?;
            if block.iter().any(|b| *b != 0) {
                return Err(bad("tar requires two zero end blocks"));
            }
            // Consume the frame to validate its footer; only full zero padding blocks are allowed.
            let mut trailing = 0usize;
            let mut buffer = [0u8; 8192];
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if buffer[..n].iter().any(|b| *b != 0) {
                    return Err(bad("nonzero data after tar terminator"));
                }
                trailing = trailing
                    .checked_add(n)
                    .ok_or_else(|| bad("padding overflow"))?;
            }
            if !trailing.is_multiple_of(512) {
                return Err(bad("unaligned tar trailing padding"));
            }
            break;
        }
        if checksums.is_some() {
            return Err(bad("checksums.json must be the last archive entry"));
        }
        let h = header(&block)?;
        if seen.len() >= limits.entries {
            return Err(Error::Limit("archive entry count".into()));
        }
        if !seen.insert(h.path.clone()) {
            return Err(bad(format!("duplicate archive path: {}", h.path)));
        }
        if manifest.is_none() && (h.path != "manifest.toml" || h.directory) {
            return Err(bad("manifest.toml must be the first entry"));
        }
        if h.size > limits.file_bytes {
            return Err(Error::Limit(format!("file size: {}", h.path)));
        }
        let declared = manifest
            .as_ref()
            .and_then(|m| m.files.iter().find(|f| f.path == h.path));
        if h.directory {
            let allowed = ["payload", "payload/bin", "payload/lib", "ui", "assets"]
                .contains(&h.path.as_str())
                || manifest.as_ref().is_some_and(|m| {
                    m.files
                        .iter()
                        .any(|f| f.path.starts_with(&format!("{}/", h.path)))
                });
            if !allowed || declared.is_some() {
                return Err(bad(
                    "directory outside canonical layout or conflicts with a file",
                ));
            }
            continue;
        }
        let maximum = match h.path.as_str() {
            "manifest.toml" | "LICENSE" => MANIFEST_LIMIT,
            "checksums.json" => 1024 * 1024,
            _ => {
                let f = declared.ok_or_else(|| bad(format!("undeclared file: {}", h.path)))?;
                if matches!(f.kind, FileKind::Ui | FileKind::Icon) {
                    1024 * 1024
                } else {
                    usize::MAX
                }
            }
        };
        if h.size > maximum as u64 {
            return Err(Error::Limit(format!("metadata/UI/icon size: {}", h.path)));
        }
        let expected_mode = if declared.is_some_and(|f| f.kind == FileKind::Executable) {
            0o755
        } else {
            0o644
        };
        if h.mode != expected_mode {
            return Err(bad(format!("mode mismatch: {}", h.path)));
        }
        let capture = if matches!(
            h.path.as_str(),
            "manifest.toml" | "LICENSE" | "checksums.json"
        ) || declared
            .is_some_and(|f| matches!(f.kind, FileKind::Ui | FileKind::Icon))
        {
            maximum
        } else {
            64 * 1024
        };
        let mut prefix = Vec::new();
        let mut hasher = Sha256::new();
        let mut left = h.size;
        let mut buffer = [0u8; 16384];
        while left > 0 {
            let n = buffer
                .len()
                .min(usize::try_from(left).unwrap_or(usize::MAX));
            input.read_exact(&mut buffer[..n])?;
            hasher.update(&buffer[..n]);
            let keep = n.min(capture.saturating_sub(prefix.len()));
            prefix.extend_from_slice(&buffer[..keep]);
            left -= n as u64;
        }
        let padding = ((512 - h.size % 512) % 512) as usize;
        input.read_exact(&mut buffer[..padding])?;
        if buffer[..padding].iter().any(|b| *b != 0) {
            return Err(bad("nonzero tar file padding"));
        }
        match h.path.as_str() {
            "manifest.toml" => {
                manifest = Some(Manifest::parse(
                    std::str::from_utf8(&prefix).map_err(|_| bad("manifest is not UTF-8"))?,
                )?)
            }
            "checksums.json" => {
                checksums = Some(
                    serde_json::from_slice(&prefix)
                        .map_err(|e| bad(format!("checksums.json: {e}")))?,
                )
            }
            "LICENSE" => {
                if std::str::from_utf8(&prefix)
                    .map_err(|_| bad("LICENSE is not UTF-8"))?
                    .trim()
                    .is_empty()
                {
                    return Err(bad("LICENSE is empty"));
                }
            }
            _ => validate_file(
                declared.expect("declaration checked above"),
                h.size,
                &prefix,
            )?,
        }
        if h.path != "checksums.json" {
            files.insert(
                h.path,
                FileInfo {
                    size: h.size,
                    sha256: format!("{:x}", hasher.finalize()),
                },
            );
        }
    }
    let manifest = manifest.ok_or_else(|| bad("missing manifest.toml"))?;
    let checksums = checksums.ok_or_else(|| bad("missing checksums.json"))?;
    if !files.contains_key("LICENSE") || manifest.files.iter().any(|f| !files.contains_key(&f.path))
    {
        return Err(bad("missing LICENSE or declared package file"));
    }
    check_hashes(&checksums, &files)?;
    Ok(Inspection{manifest,files,authenticated:false,warning:"Unsigned ZPK: hashes detect corruption, not publisher identity. Trusted local development only."})
}

fn check_hashes(checksums: &Checksums, files: &BTreeMap<String, FileInfo>) -> Result<()> {
    if checksums.schema_version != 1 || checksums.algorithm != "sha256" {
        return Err(bad("unsupported checksum schema/algorithm"));
    }
    if checksums.files.len() != files.len()
        || files
            .iter()
            .any(|(path, info)| checksums.files.get(path) != Some(&info.sha256))
    {
        return Err(Error::Hash(
            "checksum inventory must exactly cover every regular file except checksums.json".into(),
        ));
    }
    Ok(())
}

/// Reuse the exact strict checksum contract for an already installed, bounded inventory.
pub fn verify_checksums(bytes: &[u8], files: &BTreeMap<String, FileInfo>) -> Result<()> {
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Limit("checksums exceeds 1 MiB".into()));
    }
    let sums: Checksums =
        serde_json::from_slice(bytes).map_err(|e| bad(format!("checksums.json: {e}")))?;
    check_hashes(&sums, files)
}
/// Content validation without extraction/execution. Call Manifest::validate first.
pub fn validate_file_content(file: &PackageFile, data: &[u8]) -> Result<()> {
    if data.len() > 64 * 1024 * 1024
        || (matches!(file.kind, FileKind::Ui | FileKind::Icon) && data.len() > 1024 * 1024)
    {
        return Err(Error::Limit("file content".into()));
    }
    let prefix = if matches!(file.kind, FileKind::Executable | FileKind::Library) {
        &data[..data.len().min(64 * 1024)]
    } else {
        data
    };
    validate_file(file, data.len() as u64, prefix)
}

fn validate_file(file: &PackageFile, size: u64, data: &[u8]) -> Result<()> {
    match file.kind {
        FileKind::Executable | FileKind::Library => elf(file, size, data),
        FileKind::Ui => {
            let source = std::str::from_utf8(data)
                .map_err(|_| bad(format!("UI file is not UTF-8: {}", file.path)))?;
            if source.contains('\0')
                || (file.path.ends_with(".html") && !source.to_ascii_lowercase().contains("<html"))
            {
                return Err(bad(format!("invalid HTML/UI entry: {}", file.path)));
            }
            Ok(())
        }
        FileKind::Icon => icon(data),
        FileKind::Asset => Ok(()),
    }
}
fn icon(data: &[u8]) -> Result<()> {
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") || !data.ends_with(b"\0\0\0\0IEND\xaeB`\x82") {
        return Err(bad("icon must be one complete static PNG"));
    }
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits {
        bytes: 8 * 1024 * 1024,
    });
    let mut reader = decoder
        .read_info()
        .map_err(|e| bad(format!("invalid PNG: {e}")))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 1024
        || info.height > 1024
        || info.animation_control.is_some()
    {
        return Err(bad("PNG icon dimensions/APNG unsupported"));
    }
    let size = reader
        .output_buffer_size()
        .filter(|n| *n <= 8 * 1024 * 1024)
        .ok_or_else(|| bad("PNG expansion limit"))?;
    reader
        .next_frame(&mut vec![0; size])
        .map_err(|e| bad(format!("invalid PNG data: {e}")))?;
    reader
        .finish()
        .map_err(|e| bad(format!("invalid PNG end: {e}")))?;
    Ok(())
}
fn elf(file: &PackageFile, size: u64, data: &[u8]) -> Result<()> {
    let failure = || bad(format!("invalid/unsupported ELF: {}", file.path));
    if data.len() < 64 || &data[..7] != b"\x7fELF\x02\x01\x01" || ![0, 3].contains(&data[7]) {
        return Err(failure());
    }
    let u16_at = |i| u16::from_le_bytes([data[i], data[i + 1]]);
    let u64_at = |i| u64::from_le_bytes(data[i..i + 8].try_into().expect("bounded ELF field"));
    let kind = u16_at(16);
    if u16_at(18) != 62 {
        return Err(Error::UnsupportedTarget(format!(
            "ELF {} has e_machine={}, expected x86_64 (62)",
            file.path,
            u16_at(18)
        )));
    }
    if ![2, 3].contains(&kind)
        || (file.kind == FileKind::Library && kind != 3)
        || data[20..24] != [1, 0, 0, 0]
        || u16_at(52) != 64
        || u16_at(54) != 56
    {
        return Err(failure());
    }
    let offset = u64_at(32);
    let count = u16_at(56) as u64;
    let end = offset.checked_add(count * 56).ok_or_else(failure)?;
    if count == 0 || count > 128 || offset < 64 || end > data.len() as u64 {
        return Err(failure());
    }
    let mut entry_valid = false;
    let mut has_load = false;
    let mut interpreter = None;
    for i in 0..count {
        let start = (offset + i * 56) as usize;
        let p = &data[start..start + 56];
        let number =
            |at| u64::from_le_bytes(p[at..at + 8].try_into().expect("bounded program header"));
        let ty = u32::from_le_bytes(p[..4].try_into().expect("bounded type"));
        let flags = u32::from_le_bytes(p[4..8].try_into().expect("bounded flags"));
        let offset = number(8);
        let virtual_address = number(16);
        let length = number(32);
        let memory = number(40);
        if offset.checked_add(length).is_none_or(|end| end > size) {
            return Err(failure());
        }
        if ty == 1 {
            if memory < length || virtual_address.checked_add(memory).is_none() {
                return Err(failure());
            }
            has_load = true;
            if flags & 1 != 0
                && u64_at(24) >= virtual_address
                && u64_at(24) < virtual_address + length
            {
                entry_valid = true;
            }
        }
        if ty == 3 {
            if interpreter.is_some()
                || length == 0
                || length > 256
                || offset + length > data.len() as u64
            {
                return Err(failure());
            }
            interpreter = Some(&data[offset as usize..(offset + length) as usize]);
        }
    }
    if !has_load || (file.kind == FileKind::Executable && !entry_valid) {
        return Err(failure());
    }
    match (&file.linkage, interpreter) {
        (Some(Linkage::Static), None) | (None, None) => Ok(()),
        (Some(Linkage::MuslDynamic), Some(b"/lib/ld-musl-x86_64.so.1\0")) => Ok(()),
        _ => Err(bad(format!(
            "ELF interpreter/linkage mismatch: {}",
            file.path
        ))),
    }
}
