//! Owned foreground native demo. No display server, container or public network port.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    time::Duration,
};
fn main() {
    if std::env::args().nth(1).as_deref() != Some("--serve") {
        println!("Hello from native ZionOS and the Zion App SDK!");
        return;
    }
    let data = PathBuf::from(std::env::var_os("ZPK_DATA_DIR").expect("Runtime required"))
        .join("state/starts");
    let starts = fs::read_to_string(&data)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    fs::write(data, starts.to_string()).expect("private persistent data");
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
        let mut request = [0u8; 14];
        if stream.read_exact(&mut request).is_ok() && &request == b"ZPK_HEALTH_V1\n" {
            let _ = stream.write_all(b"OK\n\n");
        }
    }
}
