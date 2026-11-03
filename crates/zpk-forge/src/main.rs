use std::{env, path::PathBuf, process::ExitCode};
use zpk_forge::{
    config::{self, Project, System},
    elf, fail, read_limited, workflow, Result,
};
const HELP: &str = "ZPK Forge (build isolation required; no installation)\n\
  zpk forge init [--project DIR] [--system rust|cmake|go|make]\n\
  zpk forge build [--project DIR] [--prepare-builder]\n\
  zpk forge package [--project DIR] [--output FILE.zpk]\n\
  zpk forge inspect FILE.zpk\n\
  zpk forge validate [FILE.zpk] [--project DIR]\n\
  zpk forge clean [--project DIR]\n\
prepare-builder downloads trusted Alpine toolchains only; source compilation has no network.\n\
Unsigned ZPK is for trusted local development. CPU verification is limited; see provenance.";
fn execute() -> Result<()> {
    let mut args = env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--help") {
        println!("{HELP}");
        return Ok(());
    }
    if first.as_deref() != Some("forge") {
        return fail(HELP);
    }
    let command = args.next().ok_or(HELP)?;
    if command == "--help" {
        println!("{HELP}");
        return Ok(());
    }
    let mut project = env::current_dir()?;
    let mut system = None;
    let mut prepare = false;
    let mut output = None;
    let mut file = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => project = PathBuf::from(args.next().ok_or("--project requires a path")?),
            "--system" => {
                system = Some(match args.next().as_deref() {
                    Some("rust") => System::Rust,
                    Some("cmake") => System::Cmake,
                    Some("go") => System::Go,
                    Some("make") => System::Make,
                    _ => {
                        return fail(
                            "unsupported build system; only rust/cmake/go/make are implemented",
                        )
                    }
                })
            }
            "--prepare-builder" => prepare = true,
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output requires a path")?,
                ))
            }
            _ if !arg.starts_with('-') && file.is_none() => file = Some(PathBuf::from(arg)),
            _ => return fail("unknown/duplicate argument; use zpk forge --help"),
        }
    }
    if (system.is_some() && command != "init")
        || (prepare && command != "build")
        || (output.is_some() && command != "package")
        || (file.is_some() && !["inspect", "validate"].contains(&command.as_str()))
    {
        return fail("option does not apply to this command");
    }
    match command.as_str() {
        "init" => {
            let missing = config::init(&project, system)?;
            println!(
                "Created zpk.toml. Unresolved fields requiring developer input: {}",
                missing.join(", ")
            );
        }
        "build" => {
            let generation = workflow::build(&project, prepare)?;
            println!(
                "Built and validated staging: {}\n{}",
                generation.display(),
                elf::CPU_LIMIT
            );
        }
        "package" => {
            let path = workflow::package(&project, output.as_deref())?;
            println!("Created {}\n{}\nUnsigned trusted-local-development package; hashes do not authenticate publishers.",path.display(),elf::CPU_LIMIT);
        }
        "inspect" => {
            let file = file.ok_or("inspect requires a package path")?;
            let report = workflow::inspect_package(&read_limited(&file, 64 * 1024 * 1024)?)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "validate" => {
            if let Some(file) = file {
                workflow::inspect_package(&read_limited(&file, 64 * 1024 * 1024)?)?;
                println!("Valid ZPK archive; unsigned. {}", elf::CPU_LIMIT);
            } else {
                let config = Project::load(&project)?;
                zpk_forge::files::snapshot(&project, &config.recipe, b"")?;
                println!("Manifest, recipe and source inputs valid; no compilation or dependency resolution performed.");
            }
        }
        "clean" => {
            workflow::clean(&project)?;
            println!("Removed only marker-owned .zpk-forge outputs for this project.");
        }
        _ => return fail(HELP),
    }
    Ok(())
}
fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Forge rejected: {}", e.to_string().escape_debug());
            ExitCode::FAILURE
        }
    }
}
