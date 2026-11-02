use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};
use zpk_runtime::{
    backend::{self, NativeOpenRc, OpenRc, Operations, ServiceBackend},
    config::{Layout, RuntimeConfig},
    installed, launch, policy, Result,
};
fn execute() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command=args.next().ok_or("Usage: zpk-runtime probe|plan|prepare|verify|launch|start|stop|restart|status|healthcheck|logs|autostart --config FILE [--root DIR] [on|off]")?;
    if command == "probe" {
        println!("Landlock ABI: {}", policy::landlock_abi()?);
        return Ok(());
    }
    let mut config = None;
    let mut root = PathBuf::from("/");
    let mut enabled = None;
    let mut stderr = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config = Some(PathBuf::from(args.next().ok_or("config path required")?)),
            "--root" => root = PathBuf::from(args.next().ok_or("root path required")?),
            "on" => enabled = Some(true),
            "off" => enabled = Some(false),
            "--stderr" => stderr = true,
            _ => return Err("unknown runtime argument".into()),
        }
    }
    let path = config.ok_or("--config required")?;
    if command == "plan" {
        let config = RuntimeConfig::parse(std::str::from_utf8(&installed::read_file(
            &path,
            64 * 1024,
        )?)?)?;
        let plan = installed::load(&root, &config)?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"service":plan.layout.service,"program":plan.program,"policy_applied":false,"required_controls":policy::ENFORCED,"limitations":policy::LIMITS})
            )?
        );
        return Ok(());
    }
    if root != Path::new("/") {
        return Err("only pure plan supports a temporary root; lifecycle tests use mocks or a disposable VM".into());
    }
    // Stop/disable must remain available when current/payload/dependencies are broken.
    if command == "stop" || (command == "autostart" && enabled == Some(false)) {
        let mut c = launch::control_config(&path)?;
        let l = Layout::new(Path::new("/"), &c)?;
        if !Path::new("/etc/init.d").join(&l.service).try_exists()?
            && !l.control.join(format!("{}.pid", l.service)).try_exists()?
        {
            if command == "autostart" {
                c.autostart = false;
                launch::save_config(&path, &c)?;
            }
            return Ok(());
        }
        let result = if command == "stop" {
            NativeOpenRc.run("/sbin/rc-service", &[&l.service, "stop"])?
        } else {
            NativeOpenRc.run("/sbin/rc-update", &["del", &l.service, "default"])?
        };
        if result.code != 0 {
            return Err("OpenRC stop/disable failed".into());
        }
        if command == "autostart" {
            c.autostart = false;
            launch::save_config(&path, &c)?;
        }
        return Ok(());
    }
    let plan = launch::system_plan(&path)?;
    let backend = OpenRc {
        operations: NativeOpenRc,
    };
    match command.as_str() {
        "prepare" => {
            launch::prepare(&plan)?;
            if plan.config.autostart {
                backend.autostart(&plan, true)?;
            }
            println!(
                "Prepared native runtime; storage mode {:?}",
                plan.config.storage_mode
            );
        }
        "verify" => {
            launch::verify_persistence(&plan)?;
            policy::landlock_abi()?;
        }
        "launch" => launch::execute(&plan)?,
        "start" => backend.start(&plan)?,
        "stop" => backend.stop(&plan)?,
        "restart" => backend.restart(&plan)?,
        "status" => println!("{}", serde_json::to_string(&backend.status(&plan)?)?),
        "healthcheck" => {
            let health = backend::health(&plan, &backend.status(&plan)?)?;
            println!("{}", serde_json::to_string(&health)?);
            if !health.healthy {
                return Err("unhealthy native service".into());
            }
        }
        "logs" => println!(
            "{}",
            serde_json::json!({"stream":if stderr{"stderr"}else{"stdout"},"data":backend::logs(&plan,stderr,65536)?})
        ),
        "autostart" => {
            let enabled = enabled.ok_or("autostart requires on/off")?;
            backend.autostart(&plan, enabled)?;
            let mut config = plan.config;
            config.autostart = enabled;
            let layout = Layout::new(Path::new("/"), &config)?;
            launch::save_config(&layout.config, &config)?;
        }
        _ => return Err("unknown runtime command".into()),
    }
    Ok(())
}
fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ZPK Runtime: {}", e.to_string().escape_debug());
            ExitCode::FAILURE
        }
    }
}
