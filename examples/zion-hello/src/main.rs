//! Owned foreground native demo. No display server, container or public network port.
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    time::Duration,
};
mod settings;
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--fail-start") {
        // Explicit developer fixture: a failed candidate must never replace the healthy version.
        std::process::exit(42);
    }
    if std::env::args().nth(1).as_deref() != Some("--serve") {
        println!("Hello from native ZionOS and the Zion App SDK!");
        return;
    }
    let state =
        PathBuf::from(std::env::var_os("ZPK_DATA_DIR").expect("Runtime required")).join("state");
    let data = state.join("starts");
    let starts = match settings::read(&data) {
        Ok(s) => s.parse::<u64>().expect("valid start counter"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => panic!("persistent counter: {e}"),
    }
    .checked_add(1)
    .expect("start counter overflow");
    settings::atomic(&data, &starts.to_string()).expect("private persistent counter");
    let setting = state.join("greeting.txt");
    match settings::read(&setting) {
        Ok(s) => assert!(settings::valid(&s), "invalid persistent greeting"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            settings::atomic(&setting, "Welcome to ZionOS").expect("initialize greeting")
        }
        Err(e) => panic!("persistent greeting: {e}"),
    }
    let socket = PathBuf::from(std::env::var_os("ZPK_RUNTIME_DIR").expect("Runtime required"))
        .join("service.sock");
    let _ = fs::remove_file(&socket);
    let server = UnixListener::bind(socket).expect("private health socket");
    println!("Zion Hello native backend ready; starts={starts}");
    for connection in server.incoming() {
        let mut stream = connection.expect("connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = String::new();
        if BufReader::new((&stream).take(1025))
            .read_line(&mut request)
            .is_err()
            || request.len() > 1024
            || !request.ends_with('\n')
        {
            continue;
        }
        let response = match request.as_str() {
            "ZPK_HEALTH_V1\n" => "OK\n\n".into(),
            "ZPK_INFO_V1\n" => format!("VERSION {}\n", env!("CARGO_PKG_VERSION")),
            "ZPK_SETTINGS_V1 GET\n" => {
                format!("OK {}\n", settings::read(&setting).expect("read greeting"))
            }
            _ => match request
                .strip_prefix("ZPK_SETTINGS_V1 SET ")
                .and_then(|s| s.strip_suffix('\n'))
            {
                Some(value) if settings::valid(value) => match settings::atomic(&setting, value) {
                    Ok(()) => "OK saved\n".into(),
                    Err(_) => "ERROR persistence\n".into(),
                },
                _ => "ERROR invalid request\n".into(),
            },
        };
        let _ = stream.write_all(response.as_bytes());
    }
}
