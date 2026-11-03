use std::{path::PathBuf, process::ExitCode};
fn main() -> ExitCode {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    let result=match a.as_slice(){[c]if c=="identity"=>zpk_control::platform::identity().map(|_|()),[c]if c=="volume"=>zpk_control::platform::volume(),[c]if c=="restore"=>zpk_control::platform::restore(),[c]if c=="serve"=>zpk_control::platform::serve(),[c]if c=="bootstrap"=>zpk_control::platform::bootstrap(None),[c,f]if c=="bootstrap-file"=>zpk_control::platform::bootstrap(Some(PathBuf::from(f))),[c,f]if c=="catalog-import"=>zpk_control::platform::import_catalog(&PathBuf::from(f)),_=>Err("Usage (target root only): zion-platform identity|volume|restore|serve|bootstrap|bootstrap-file TOKEN_FILE|catalog-import FILE.zpk".into())};
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Zion platform: {}", e.to_string().escape_debug());
            ExitCode::FAILURE
        }
    }
}
