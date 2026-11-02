use crate::{fail, Result};
#[derive(Debug, Clone, Copy)]
pub struct Adapter {
    pub architecture: &'static str,
    pub supported: bool,
    pub toolchain: &'static str,
}
pub const ADAPTERS: [Adapter; 4] = [
    Adapter {
        architecture: "x86_64",
        supported: true,
        toolchain: "Alpine musl; rustc/gcc/g++/Go amd64 v1",
    },
    Adapter {
        architecture: "aarch64",
        supported: false,
        toolchain: "requires tested cross toolchain and packaging fixtures",
    },
    Adapter {
        architecture: "i686",
        supported: false,
        toolchain: "requires tested 32-bit musl toolchain and ELF validation",
    },
    Adapter {
        architecture: "armv7",
        supported: false,
        toolchain: "requires tested ARMv7 musl toolchain/ABI and ELF validation",
    },
];
pub fn resolve(architecture: &str) -> Result<Adapter> {
    match ADAPTERS.iter().find(|a| a.architecture == architecture) {
        Some(a) if a.supported => Ok(*a),
        Some(a) => fail(format!(
            "unsupported target {}: {}",
            a.architecture, a.toolchain
        )),
        None => fail(format!("unsupported architecture: {architecture}")),
    }
}
