//! Unprivileged browser fixture coordinator. No account, process, mount or service operations.
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
use zpk_manager::{
    filesystem::Environment,
    host::Host,
    manager::{Manager, NoFaults},
    model::Review,
    Result,
};
use zpk_runtime::{
    backend::{ServiceBackend, State, Status},
    config::{Identity, Plan},
};
struct Mock;
impl ServiceBackend for Mock {
    fn start(&self, _: &Plan) -> Result<()> {
        Err("browser fixtures never execute package content".into())
    }
    fn stop(&self, _: &Plan) -> Result<()> {
        Ok(())
    }
    fn restart(&self, _: &Plan) -> Result<()> {
        Err("browser fixtures never execute package content".into())
    }
    fn status(&self, p: &Plan) -> Result<Status> {
        Ok(Status {
            state: if p.program.is_none() {
                State::NotRequired
            } else {
                State::Stopped
            },
            pid: None,
            restart_count: None,
            backend_code: 3,
        })
    }
    fn autostart(&self, _: &Plan, _: bool) -> Result<()> {
        Err("mock runlevels are unavailable".into())
    }
}
impl Host for Mock {
    fn identity(&self, id: &str, reserved: &BTreeSet<u32>) -> Result<Identity> {
        let uid = (15000..60000)
            .find(|id| !reserved.contains(id))
            .ok_or("identity budget")?;
        Ok(Identity {
            name: zpk_runtime::config::identity_name(id),
            uid,
            gid: uid,
        })
    }
    fn register(&self, p: &Plan) -> Result<()> {
        for dir in [&p.layout.data, &p.layout.logs] {
            fs::create_dir_all(dir)?;
        }
        for slot in &p.manifest.storage {
            fs::create_dir_all(
                if matches!(slot.kind, zpk_schema::manifest::StorageKind::Data) {
                    p.layout.data.join(&slot.name)
                } else {
                    p.layout.configuration.join(&slot.name)
                },
            )?;
        }
        Ok(())
    }
    fn unregister(&self, _: &Plan) -> Result<()> {
        Ok(())
    }
    fn healthy(&self, _: &Plan) -> Result<bool> {
        Err("native health is not emulated by browser fixtures".into())
    }
    fn logs(&self, _: &Plan, _: bool) -> Result<String> {
        Err("native logs are not emulated by browser fixtures".into())
    }
}
fn execute() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || unsafe { libc::geteuid() } == 0 {
        return Err("Usage: non-root zpk-sdk-fixture PRIVATE_TEMP_ROOT install|uninstall|enable|disable PACKAGE_OR_ID".into());
    }
    let root = PathBuf::from(&args[0]);
    let uid = unsafe { libc::geteuid() };
    if !root.is_absolute()
        || fs::canonicalize(&root)? != root
        || root.parent() != Some(fs::canonicalize(std::env::temp_dir())?.as_path())
        || !root
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("zion-sdk-"))
        || fs::metadata(&root)?.uid() != uid
        || fs::metadata(&root)?.mode() & 0o077 != 0
    {
        return Err("fixture requires a private owned zion-sdk-* directory directly under the temporary directory".into());
    }
    fs::create_dir_all(root.join("etc"))?;
    fs::write(root.join("etc/zionos-release"), "ZionOS Genesis 0.6.1\n")?;
    let mut manager = Manager::open(&root, Environment::TemporaryTest, Mock)?;
    manager.reconcile()?;
    let command = args[1].to_str().ok_or("command encoding")?;
    let operand = args[2].to_str().ok_or("operand encoding")?;
    match command {
        "install" => {
            let p = Path::new(operand);
            let data = zpk_runtime::installed::read_file(p, 64 * 1024 * 1024)?;
            let m = zpk_schema::inspect(std::io::Cursor::new(data), zpk_schema::Limits::default())?
                .manifest;
            let r = manager.install(
                p,
                false,
                &Review {
                    desktop_entry: None,
                    trusted_local_development: true,
                    approved_permissions: m.permissions,
                    start: false,
                    autostart: false,
                    unix_health: false,
                },
                &NoFaults,
            )?;
            if r.desktop_registry != "published" {
                return Err(r.desktop_registry.into());
            }
        }
        "uninstall" => {
            manager.uninstall(operand, &NoFaults)?;
        }
        "enable" | "disable" => manager.desktop_enabled(operand, command == "enable")?,
        _ => return Err("unsupported fixture operation".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("SDK fixture: {}", e.to_string().escape_debug());
        std::process::exit(1);
    }
}
