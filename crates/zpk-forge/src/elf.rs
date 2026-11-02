use crate::{fail, Result};
use goblin::elf::Elf;
use serde::{Deserialize, Serialize};
pub const CPU_LIMIT:&str="LIMITED CPU VERIFICATION: enforced x86-64 build flags and ELF ISA-needed notes; no complete instruction/control-flow proof. Sources may override flags or contain runtime-dispatched instructions. Test actual legacy hardware before deployment.";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependencies {
    pub file: String,
    pub needed: Vec<String>,
    pub interpreter: Option<String>,
    pub runpaths: Vec<String>,
}
pub fn library_name(name: &str) -> bool {
    name.starts_with("lib")
        && name.contains(".so")
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}
pub fn inspect(path: &str, data: &[u8]) -> Result<Dependencies> {
    let elf = Elf::parse(data).map_err(|e| format!("invalid ELF {path}: {e}"))?;
    if elf.header.e_machine != 62 || !elf.is_64 || !elf.little_endian {
        return fail(format!(
            "unsupported ELF architecture for {path}; expected x86_64 ELF64 LE"
        ));
    }
    if elf
        .interpreter
        .is_some_and(|p| p != "/lib/ld-musl-x86_64.so.1")
    {
        return fail("non-musl ELF interpreter");
    }
    if elf.libraries.len() > 64 || elf.libraries.iter().any(|name| !library_name(name)) {
        return fail("unsafe/too many ELF dynamic dependencies");
    }
    for notes in [
        elf.iter_note_headers(data),
        elf.iter_note_sections(data, None),
    ]
    .into_iter()
    .flatten()
    {
        for note in notes {
            let note = note?;
            if note.name == "GNU" && note.n_type == 5 {
                check_properties(note.desc)?;
            }
        }
    }
    let runpaths: Vec<_> = elf
        .rpaths
        .iter()
        .chain(&elf.runpaths)
        .map(|s| s.to_string())
        .collect();
    if runpaths
        .iter()
        .any(|s| !["$ORIGIN", "$ORIGIN/../lib"].contains(&s.as_str()))
    {
        return fail(format!("unsafe/unsupported ELF RPATH/RUNPATH in {path}"));
    }
    Ok(Dependencies {
        file: path.into(),
        needed: elf.libraries.iter().map(|s| s.to_string()).collect(),
        interpreter: elf.interpreter.map(str::to_owned),
        runpaths,
    })
}
pub fn check_properties(mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        if bytes.len() < 8 {
            return fail("malformed GNU ELF property");
        }
        let kind = u32::from_le_bytes(bytes[..4].try_into()?);
        let size = u32::from_le_bytes(bytes[4..8].try_into()?) as usize;
        let end = 8usize.checked_add(size).ok_or("ELF property overflow")?;
        if end > bytes.len() {
            return fail("truncated GNU ELF property");
        }
        if kind == 0xc0008002 {
            // GNU_PROPERTY_X86_ISA_1_NEEDED
            if size != 4 {
                return fail("malformed x86 ISA requirement");
            }
            let flags = u32::from_le_bytes(bytes[8..12].try_into()?);
            if flags & !1 != 0 {
                return fail("unsupported CPU requirement: ELF requires x86-64-v2/v3/v4 or unknown ISA flags");
            }
        }
        let next = end
            .checked_add(7)
            .ok_or("ELF property alignment overflow")?
            & !7;
        if next > bytes.len() {
            return fail("truncated GNU ELF property padding");
        }
        bytes = &bytes[next..];
    }
    Ok(())
}
pub fn platform_library(name: &str) -> bool {
    name == "libc.musl-x86_64.so.1"
}
