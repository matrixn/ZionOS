# ZPK format v1

Normative for the current implementation. The authoring filename is `zpk.toml`;
its exact bytes become `manifest.toml` inside a `.zpk` file. Schema version is the
integer `1`. Unknown versions fail explicitly before v1 field deserialization.
Unknown fields, duplicate TOML keys, unknown enum variants and missing required
fields fail; documented optional arrays/defaults alone may be omitted.

## Archive

One ordinary **zstd frame** containing a **POSIX USTAR** archive. No concatenated or
skippable frames, dictionaries, PAX/GNU extensions, sparse records or external data.
Entry order is `manifest.toml` first, optional directories and all remaining regular
files next, `checksums.json` last. `LICENSE` is required. Optional directories may
be omitted; empty standard directories may be included.

```text
manifest.toml               # UTF-8, schema_version = 1
payload/bin/<executable>    # native Linux x86-64 ELF, one or more
payload/lib/<library>       # optional x86-64 ET_DYN ELF
ui/<entry.html>             # optional UTF-8 HTML/CSS/JS and other text assets
assets/<file>               # optional assets; icons are static PNG
LICENSE                     # nonempty UTF-8 license text
checksums.json              # final entry, exact inventory SHA-256
```

Every regular file under payload/ui/assets appears exactly once in manifest `files`.
Metadata files are reserved and not listed in `files`. Each `files` declaration must
exist in the archive. Extra files and duplicate archive paths are rejected.

Paths are relative, case-sensitive ASCII, at most 240 bytes, components at most 100
bytes from `[A-Za-z0-9._+-]`, separated by one `/`. `+` supports native library names
such as `libstdc++.so.6`. No empty, `.` or `..` components,
absolute paths, controls, colon, backslash or trailing slash on regular files.
USTAR prefix/name fields combine into the same canonical path before validation.
Directory headers may end with one `/`; their normalized names must also be unique.
A file cannot also be an ancestor directory. All symlinks and hardlinks are rejected,
including links that appear safe; v1 has no link resolution rules.

USTAR numeric fields use octal, not base-256. Header checksum, magic/version, sizes,
zero data padding and two zero end blocks are checked. Additional padding must be
zero and a multiple of 512 bytes, within the expansion budget. UID/GID/device numbers
must be zero. Mode is exactly `0755` for executable files/directories and `0644` for
other files. Setuid/setgid, world-writable files, devices and FIFOs are forbidden.
Names/times/ownership never cause host metadata changes because inspection does not
extract anything. Tar timestamps are parsed but need not be zero; builders should
use stable timestamps for deterministic authoring.

`checksums.json` is UTF-8 JSON:

```json
{
  "schema_version": 1,
  "algorithm": "sha256",
  "files": {
    "manifest.toml": "<64 lowercase hex characters>",
    "LICENSE": "<64 lowercase hex characters>",
    "payload/bin/hello": "<64 lowercase hex characters>"
  }
}
```

Hash raw file bytes, including manifest and license. Exclude directory entries and
`checksums.json` itself. The map must exactly equal the inspected regular-file
inventory; no missing/extra/self entries, duplicate JSON keys, noncanonical paths,
uppercase/nonhex/truncated digests or unknown checksum fields are accepted. Hashes
do not authenticate the publisher; v1 contains no signature mechanism.

## Manifest tables

The complete minimal manifest is [zpk-hello/zpk.toml](../../examples/zpk-hello/zpk.toml).
Service and single/multiple desktop examples are in
[valid fixtures](../../tests/zpk/fixtures/valid).

| Table / field | Contract |
|---|---|
| `schema_version` | Required integer 1 |
| `package.id` | Unique by convention reverse-domain name, at least 3 lowercase components; each starts with a letter, ends alphanumeric, internal lowercase letters/digits/hyphens; max 253 bytes, component max 63. Registry uniqueness is not verified offline |
| `package.name`, `description`, `author`, `license` | Required nonempty text without controls, max 128/4096/256/128 bytes. License is a nonempty label/expression, not an SPDX database validation; LICENSE contains actual terms |
| `package.version` | Strict SemVer version; prerelease/build metadata supported |
| `package.categories` | Required nonempty unique category list |
| `target` | `os="linux"`, `architecture="x86_64"`, `cpu="x86-64-v1"`, `libc="musl"`; anything else explicitly unsupported in this MVP |
| `build.system` | `rust`, `go`, `make`, `cmake`, `meson`, `custom`; declaration support does not imply a Forge adapter (Meson remains unsupported) |
| `build.toolchain`, `reproducible` | Nonempty bounded text describing compiler/target; required boolean declaration, not independent build attestation |
| `runtime.dependencies` | Required array of `{id, version}`; reverse-domain IDs and SemVer requirements, max 128 unique dependencies, no self dependency. May be empty |
| `compatibility.min_zionos` | Minimum ZionOS SemVer |
| `compatibility.upgrades_from` | SemVer requirement; fresh install does not check it; upgrades require an older installed version matching it |
| `compatibility.data_schema` | Positive integer, declared package-level persistent-data compatibility marker |
| `compatibility.conflicts`, `replaces` | Required unique ID arrays (may be empty), max 128 each, no self reference; dependency cannot also be a conflict. No resolution or removal is performed |
| `permissions` | Required unique list of known capabilities, may be empty |
| `files` | 1..2048 `{path, kind, linkage?, permissions?}` declarations, globally unique paths |
| `files.kind` | `executable` under payload/bin (linkage required), `library` under payload/lib, `ui` under ui, `icon` under assets ending .png, `asset` under assets. Linkage forbidden for non-executables |
| `files.linkage` | `static` or `musl-dynamic` |
| `files.permissions` | Defaults empty, must be unique subset of package permissions |
| `service` | Optional one `{executable, arguments, run_as, autostart, permissions}`; executable references declared native executable; `run_as="package"` only. At most 64 literal nonempty arguments, each max 1024 bytes, no controls. Requires global background-service permission |
| `desktop` | Optional array, zero to 32 entries with unique local IDs; examples cover zero, one and multiple entries |
| `desktop` entry | `{id, name, entry, icon, executable?, categories, permissions, window}`. Entry references declared ui .html; icon references declared icon .png; optional executable references a declared executable |
| `window` | Required width/height/min_width/min_height/resizable. Minimum width 160..7680, minimum height 120..4320; initial dimensions at least minima, at most 7680×4320 |
| `storage` | Optional up to 32 `{name, kind, schema, max_bytes, preserve_on_upgrade}`. Unique local name, kind `data`/`config`, positive schema/quota, required preservation declaration. No absolute host paths or actual directory allocation |

Local desktop/storage IDs start with a lowercase letter, contain only lowercase
letters/digits/hyphens, and are at most 64 bytes. Categories (1..16 unique) are:
`utilities`, `development`, `multimedia`, `network`, `storage`, `backup`, `productivity`,
`system`. Desktop names are nonempty bounded text (128 bytes).

Permissions: `desktop-ui`, `background-service`, `persistent-data`,
`configuration-write`, `network-client`, `network-listen`. Declared file/service/UI
permissions must be subsets of global permissions. Desktop entries require desktop-ui;
service requires background-service; storage data/config require persistent-data /
configuration-write. Unknown or undeclared permissions fail validation. Unused
declarations are permitted. These are declarations, not a runtime sandbox: offline
inspection cannot prove that a binary will refrain from undeclared operations.

## Content checks

Executables must have mode 0755, ELF64 little-endian version 1, x86-64 e_machine 62,
System V/Linux ABI, ET_EXEC or ET_DYN, valid 64-byte ELF header, up to 128 56-byte
program headers entirely in the first 64 KiB. Load segments must be file-bounded,
have non-overflowing virtual ranges and memsz ≥ filesz; executable entry must be in
an executable, file-backed load range. Static binaries have no PT_INTERP. Dynamic
binaries have exactly `/lib/ld-musl-x86_64.so.1` plus NUL as PT_INTERP. Libraries must
be ET_DYN and have no interpreter. Shell scripts are not native executables in v1.

This is structural ELF validation, not a complete kernel loader, malware scanner,
instruction decoder or dependency solver. It does not prove that a claimed v1
binary contains no AVX/SSE4.2 instructions, nor resolve DT_NEEDED or validate every
ELF section. Build for `x86-64` explicitly and test intended hardware separately.

UI files must be UTF-8 without NUL; .html files must contain an HTML root marker.
This verifies a local entry document, not HTML/JS safety, network capability usage
or complete asset references. Icons are fully decoded static PNG, dimensions 1..1024
per axis, bounded decode memory; SVG and APNG icons are unsupported. Generic assets
are hashed opaque bytes. No assets are rendered during inspection.

## Default resource limits

| Resource | Limit |
|---|---|
| Compressed input | 64 MiB |
| Entire expanded tar including headers/padding | 256 MiB |
| Individual regular file | 64 MiB |
| Archive entries including directories/metadata | 4096 |
| Manifest and LICENSE individually | 64 KiB |
| checksums.json, each UI file, each icon | 1 MiB |
| zstd decoder window | 8 MiB |
| PNG decode budget | 8 MiB, at most 1024×1024 pixels |

Library callers may choose compressed/expanded/file/count limits via `Limits`;
fixed metadata/decoder limits remain enforced. Limits must all be positive.
Payloads are hashed in 16 KiB chunks, retaining up to 64 KiB per ELF or a bounded
metadata/UI/icon buffer; payload content is discarded after validation. The returned
report contains declarations and file size/hash metadata, not executable content.
No file path controls a filesystem write. A `Read` implementation can block, so an
embedding application must impose its own source/time/I/O cancellation policy;
byte limits do not constitute a wall-clock deadline.

Implementation references: [Serde-compatible TOML](https://docs.rs/toml/1.1.8/toml/)
and [zstd decoder frame/window API](https://docs.rs/zstd/0.13.4/zstd/stream/read/struct.Decoder.html).
