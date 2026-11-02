fn main() {
    if std::env::args().nth(1).as_deref() == Some("--serve") {
        use std::{
            io::{Read, Write},
            os::unix::net::UnixListener,
        };
        let state =
            std::path::PathBuf::from(std::env::var_os("ZPK_DATA_DIR").expect("Runtime required"))
                .join("state/starts");
        let count = std::fs::read_to_string(&state)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        std::fs::write(state, count.to_string()).expect("private persistent state");
        let socket = std::path::PathBuf::from(
            std::env::var_os("ZPK_RUNTIME_DIR").expect("Runtime required"),
        )
        .join("service.sock");
        let _ = std::fs::remove_file(&socket);
        let server = UnixListener::bind(socket).expect("private socket");
        println!("hello Rust service ready");
        for connection in server.incoming() {
            let mut connection = connection.expect("connection");
            connection
                .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                .unwrap();
            let mut request = [0u8; 14];
            if connection.read(&mut request).is_ok() {
                let _ = connection.write_all(b"OK\n\n");
            }
        }
        return;
    }
    println!("Hello from native ZionOS Rust, built with Forge!");
}
