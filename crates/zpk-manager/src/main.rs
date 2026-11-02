use std::{path::Path, process::ExitCode};
use zpk_manager::{
    fail,
    filesystem::Environment,
    host::Alpine,
    manager::{Manager, NoFaults},
    model::Review,
    Result,
};
const HELP:&str="Zion Package Manager — target Alpine administrator CLI; no HTTP installer\n\
zpk install|upgrade FILE.zpk --trust-local-development --permissions LIST [--start] [--autostart] [--unix-health] [--desktop-entry ID]\n\
zpk list\nzpk info|start|stop|restart|status|logs|rollback|uninstall|verify ID\n\
zpk logs ID --stderr\nzpk repair --plan ID\n\
LIST is comma-separated manifest permissions; use 'none' for an empty set.\n\
Unsigned packages are trusted local development only. Hashes do not authenticate publishers.\n\
Uninstall preserves data/config/logs and locked identities. Data deletion is not implemented.\n\
Forge remains available via its existing crate; a separately installed /usr/local/libexec/zpk-forge may serve 'zpk forge'.";
fn execute() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    if args[0] == "forge" {
        if unsafe { libc::geteuid() } == 0 {
            return fail("run Forge as an unprivileged developer, never through the privileged package helper");
        }
        let path = Path::new("/usr/local/libexec/zpk-forge");
        zpk_runtime::installed::secure_path(Path::new("/"), path)?;
        if !std::process::Command::new(path)
            .args(&args)
            .status()?
            .success()
        {
            return fail("Forge failed");
        }
        return Ok(());
    }
    let verb = args.remove(0);
    let repair = verb == "repair";
    if repair {
        if args.first().map(String::as_str) != Some("--plan") {
            return fail("only repair --plan is implemented; no arbitrary repair scripts");
        }
        args.remove(0);
    }
    let mut review = Review {
        desktop_entry: None,
        trusted_local_development: false,
        approved_permissions: vec![],
        start: false,
        autostart: false,
        unix_health: false,
    };
    let mut operand = None;
    let mut stderr = false;
    let mut permissions_seen = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--desktop-entry" if review.desktop_entry.is_none() => {
                i += 1;
                review.desktop_entry =
                    Some(args.get(i).ok_or("--desktop-entry requires ID")?.clone());
            }
            "--trust-local-development" if !review.trusted_local_development => {
                review.trusted_local_development = true
            }
            "--permissions" if !permissions_seen => {
                i += 1;
                let list = args.get(i).ok_or("--permissions requires LIST")?;
                permissions_seen = true;
                if list != "none" {
                    for p in list.split(',') {
                        review
                            .approved_permissions
                            .push(serde_json::from_value(serde_json::Value::String(p.into()))?);
                    }
                }
            }
            "--start" if !review.start => review.start = true,
            "--autostart" if !review.autostart => review.autostart = true,
            "--unix-health" if !review.unix_health => review.unix_health = true,
            "--stderr" if !stderr => stderr = true,
            a if !a.starts_with('-') && operand.is_none() => operand = Some(a.to_owned()),
            _ => return fail("unknown/duplicate argument; use zpk --help"),
        }
        i += 1;
    }
    let installing = verb == "install" || verb == "upgrade";
    if (!installing
        && (review.trusted_local_development
            || permissions_seen
            || review.start
            || review.autostart
            || review.unix_health
            || review.desktop_entry.is_some()))
        || (stderr && verb != "logs")
        || (verb == "list" && operand.is_some())
        || (!installing
            && verb != "list"
            && operand
                .as_deref()
                .is_none_or(|id| !zpk_schema::manifest::package_id(id)))
        || (installing
            && (!permissions_seen || !review.trusted_local_development || operand.is_none()))
    {
        return fail("invalid arguments or missing explicit development trust/permission review; use zpk --help");
    }
    if ![
        "install",
        "upgrade",
        "list",
        "info",
        "start",
        "stop",
        "restart",
        "status",
        "logs",
        "rollback",
        "uninstall",
        "verify",
        "repair",
    ]
    .contains(&verb.as_str())
    {
        return fail("unsupported manager command");
    }
    if unsafe { libc::geteuid() } != 0
        || !Path::new("/etc/alpine-release").is_file()
        || !Path::new("/etc/zionos-release").is_file()
        || !Path::new("/sbin/openrc-run").is_file()
    {
        return fail("manager administration is only supported by root on the target Alpine/ZionOS system; never deploy packages on the Windows/WSL developer host");
    }
    let mut m = Manager::open(Path::new("/"), Environment::Persistent, Alpine)?;
    // A requested repair plan never triggers filesystem/service reconciliation.
    if !repair {
        for message in m.reconcile()? {
            eprintln!("{message}");
        }
    }
    let id = operand.as_deref().unwrap_or("");
    let output = match verb.as_str() {
        "install" | "upgrade" => {
            eprintln!("WARNING: unsigned trusted-local development package. SHA-256 does not authenticate its publisher. Approved declarations: {:?}; no data migration hooks will execute.",review.approved_permissions);
            serde_json::to_value(m.install(
                Path::new(id),
                verb == "upgrade",
                &review,
                &NoFaults,
            )?)?
        }
        "list" => serde_json::to_value(m.list()?)?,
        "info" => serde_json::to_value(m.info(id)?)?,
        "start" | "stop" | "restart" | "status" => serde_json::to_value(m.lifecycle(id, &verb)?)?,
        "logs" => {
            serde_json::json!({"stream":if stderr{"stderr"}else{"stdout"},"data":m.logs(id,stderr)?})
        }
        "rollback" => serde_json::to_value(m.rollback(id, &NoFaults)?)?,
        "uninstall" => serde_json::to_value(m.uninstall(id, &NoFaults)?)?,
        "verify" => {
            m.verify(id)?;
            serde_json::json!({"id":id,"verified":true})
        }
        "repair" => m.repair_plan(id)?,
        _ => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ZPK Manager: {}", e.to_string().escape_debug());
            ExitCode::FAILURE
        }
    }
}
