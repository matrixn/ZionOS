use semver::Version;
use std::{env, fs::File, process::ExitCode};
use zpk_schema::{inspect, Limits};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err(
            "Usage: zpk-inspect FILE.zpk ZIONOS_VERSION [INSTALLED_PACKAGE_VERSION]".into(),
        );
    }
    let zionos = Version::parse(&args[1])?;
    let previous = args.get(2).map(|v| Version::parse(v)).transpose()?;
    let file = File::open(&args[0])?;
    if !file.metadata()?.is_file() {
        return Err("Input must be a regular local ZPK file".into());
    }
    let report = inspect(file, Limits::default())?;
    report
        .manifest
        .check_compatibility(&zionos, previous.as_ref())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ZPK rejected: {error}");
            ExitCode::FAILURE
        }
    }
}
