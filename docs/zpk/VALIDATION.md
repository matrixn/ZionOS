# Validation and security boundaries

The input archive and every manifest/metadata field are untrusted. Inspection runs
as an ordinary developer user against a local regular file, in-process, and does
not extract archives, spawn payloads, invoke a shell for package arguments, install
services, grant permissions or render HTML/icons. The library accepts `Read` so tests
can generate adversarial archives in memory.

## Fail-closed checks

1. Limit compressed bytes and zstd window. Accept exactly one ordinary zstd frame.
2. Limit expanded bytes including tar padding. Verify USTAR headers, checksum,
   modes/ownership/types, canonical paths and unique entries before using content.
3. Require a small, first-entry v1 manifest with strict Serde/TOML types and known
   capabilities; explicitly reject unsupported OS/architecture/CPU/libc.
4. Require each package file to be declared in the expected layout. Reject file/
   directory aliases, all links and extension records before following anything.
5. Stream SHA-256; perform bounded native ELF, UTF-8 UI and static PNG checks. Require
   LICENSE and every declared file. Nothing is executed or extracted.
6. Require final strict checksums.json with unique keys and exact content inventory.
   Consume zero tar termination/padding and zstd frame end; reject truncation,
   appended bytes and concatenated frames.
7. Separately check caller-provided OS version and, for upgrades, the old version.
   Matching a SemVer requirement does not prove data migration compatibility.

Checksums.json cannot hash itself. Manifest and LICENSE are included. Signature
validation is deliberately absent. **A successfully validated unsigned archive is
only suitable for trusted local development; it is not authenticated software.**
Do not use this inspector as an installer or bypass OS permission checks based on
its result. A future extractor needs its own directory-FD/openat-style isolation,
atomic staging/rollback and capacity checks even though this version rejects links.

## Regression evidence

Integration tests live in `tests/zpk/validation.rs` (wired through crate Cargo.toml).
They generate regular/directory/link/special-file/extension headers, unsafe names,
duplicate paths, invalid metadata, hash maps, bounded compression bombs and malformed
ELF/PNG/UI entirely in memory. CLI tests write only disposable ordinary temp files.
Fixtures include a CLI-only manifest, service/storage declarations, one desktop entry,
multiple desktop entries, and invalid TOML cases.

Test classes cover:

- Strict reverse-domain IDs, SemVer/requirements, unknown schema/fields/permissions,
  duplicate declarations, invalid feature references, windows, quotas and service user.
- OS and upgrade boundaries, unsupported architecture/CPU errors.
- Traversal, absolute/Windows/alias paths, prefix/name interpretation, links,
  file/directory collisions, privileged modes and ownership, tar checksum/magic/numbers.
- Missing/extra files, inventory mismatch, hash mutation, malformed/duplicate checksum
  JSON, unsupported checksum schema/algorithm, ordering, termination and padding.
- Zstd truncation, frame concatenation/trailing data/window limit, compressed/expanded/
  per-file/entry limits, and a reader returning a single byte per call.
- ELF headers/machine/program-table ranges/entry/linkage and library declarations;
  icon decode failure, invalid UTF-8/HTML and missing desktop assets.
- Inspector CLI version failures and explicit unsigned report.

CI also compiles the owned native hello source for musl/x86-64 baseline, builds a
real archive, and inspects it. It does not execute the generated application. Genesis
keeps its existing separate CI job. Lockfiles are committed separately and commands
use `--locked`. See [verification record](VERIFICATION.md) for this task's results.

## Deliberate limits

No signature trust, repository, installation, package database, dependency resolution,
ELF DT_NEEDED resolution, CPU instruction verification, isolation enforcement, UI
sandbox, migration, service manager integration or rollback is implemented. HTML
can contain active or remote content; this tool never opens it. Declaring permissions
does not constrain executable syscalls. License labels are not independently verified.
Malformed structures are rejected, but complete semantic validation of machine code
and every ELF section is outside this MVP. It is not a security certification.

Byte/pixel/window/count limits bound archive expansion and allocations. Streaming
reads can still block on their source; future API consumers must arrange cancellation
and deadlines. There is no HTTP route for package inspection in this task.
