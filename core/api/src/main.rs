mod access;
mod apps;
mod cache;
mod lab;
mod packages;
mod pools;
mod provisioning;
mod storage;

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::{fs, net::SocketAddr, path::Path, time::Duration};
use tokio::net::TcpListener;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct Disk {
    name: String,
    size_bytes: u64,
    model: String,
    removable: bool,
    rotational: bool,
    read_only: bool,
}

#[derive(Serialize)]
struct Metrics {
    hostname: String,
    kernel: String,
    architecture: &'static str,
    uptime_seconds: f64,
    memory_total_bytes: u64,
    memory_available_bytes: u64,
    cpu_cores: usize,
    cpu_model: String,
    cpu_usage_percent: Option<f64>,
    disks: Vec<Disk>,
}

fn read_trimmed(path: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn parse_meminfo_kb(meminfo: &str, key: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name != key {
            return None;
        }
        value.split_whitespace().next()?.parse().ok()
    })
}

fn parse_cpu_times(stat: &str) -> Option<(u64, u64)> {
    let mut values = stat.lines().next()?.split_whitespace();
    if values.next()? != "cpu" {
        return None;
    }
    let ticks: Vec<u64> = values.map(str::parse).collect::<Result<_, _>>().ok()?;
    if ticks.len() < 5 {
        return None;
    }
    // guest and guest_nice are already included in user and nice respectively.
    let total = ticks
        .iter()
        .take(8)
        .try_fold(0_u64, |sum, value| sum.checked_add(*value))?;
    let idle = ticks[3].saturating_add(ticks[4]);
    Some((total, idle))
}

fn cpu_usage(first: (u64, u64), second: (u64, u64)) -> Option<f64> {
    let delta_total = second.0.checked_sub(first.0)?;
    if delta_total == 0 {
        return None;
    }
    let delta_idle = second.1.saturating_sub(first.1).min(delta_total);
    Some(100.0 * (1.0 - delta_idle as f64 / delta_total as f64))
}

fn cpu_model(cpuinfo: &str) -> String {
    cpuinfo
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim() == "model name" || name.trim() == "Hardware")
                .then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| "Unknown CPU".to_owned())
}

fn scan_disks() -> Vec<Disk> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return result;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if ["loop", "ram", "zram", "fd"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }

        let root = entry.path();
        let number = |file: &str| -> u64 {
            fs::read_to_string(root.join(file))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0)
        };
        let model = fs::read_to_string(root.join("device/model"))
            .unwrap_or_default()
            .trim()
            .to_owned();

        result.push(Disk {
            name,
            size_bytes: number("size").saturating_mul(512),
            model,
            removable: number("removable") != 0,
            rotational: number("queue/rotational") != 0,
            read_only: number("ro") != 0,
        });
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "zion-core",
        "version": VERSION,
        "readonly": true
    }))
}

async fn metrics() -> Json<Metrics> {
    let first = parse_cpu_times(&read_trimmed("/proc/stat"));
    tokio::time::sleep(Duration::from_millis(250)).await;
    let second = parse_cpu_times(&read_trimmed("/proc/stat"));
    let cpu_usage_percent = match (first, second) {
        (Some(a), Some(b)) => cpu_usage(a, b),
        _ => None,
    };

    let meminfo = read_trimmed("/proc/meminfo");
    let cpuinfo = read_trimmed("/proc/cpuinfo");
    let uptime_seconds = read_trimmed("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    Json(Metrics {
        hostname: read_trimmed("/etc/hostname"),
        kernel: read_trimmed("/proc/sys/kernel/osrelease"),
        architecture: std::env::consts::ARCH,
        uptime_seconds,
        memory_total_bytes: parse_meminfo_kb(&meminfo, "MemTotal").unwrap_or(0) * 1024,
        memory_available_bytes: parse_meminfo_kb(&meminfo, "MemAvailable").unwrap_or(0) * 1024,
        cpu_cores: cpuinfo
            .lines()
            .filter(|line| line.starts_with("processor") && line.contains(':'))
            .count(),
        cpu_model: cpu_model(&cpuinfo),
        cpu_usage_percent,
        disks: scan_disks(),
    })
}

// Static assets are served from the live ISO, or from the project folder in WSL.
// There is no user-controlled file path in these routes.
fn read_web_asset(filename: &str) -> Option<Vec<u8>> {
    let runtime_dir =
        std::env::var("ZION_WEB_DIR").unwrap_or_else(|_| "/opt/zionos/web".to_owned());
    fs::read(Path::new(&runtime_dir).join(filename))
        .or_else(|_| {
            fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../web/desktop")
                    .join(filename),
            )
        })
        .ok()
}

fn serve_asset(filename: &str, content_type: &'static str) -> Response {
    match read_web_asset(filename) {
        Some(content) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            content,
        )
            .into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "ZionOS web asset missing",
        )
            .into_response(),
    }
}

async fn storage_info() -> Json<storage::StorageSnapshot> {
    Json(storage::snapshot().await)
}

async fn pools_info() -> Json<pools::PoolSnapshot> {
    Json(pools::snapshot().await)
}

async fn provisioning_info() -> Json<provisioning::LabSnapshot> {
    Json(provisioning::snapshot_cached().await)
}

async fn index() -> Response {
    let mut response = serve_asset("index.html", "text/html; charset=utf-8");
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

async fn app_js() -> Response {
    serve_asset("app.js", "application/javascript; charset=utf-8")
}
async fn app_bridge_js() -> Response {
    serve_asset("apps/bridge.js", "application/javascript; charset=utf-8")
}
async fn app_registry_js() -> Response {
    serve_asset("apps/registry.js", "application/javascript; charset=utf-8")
}
async fn package_center_js() -> Response {
    serve_asset(
        "apps/package-center.js",
        "application/javascript; charset=utf-8",
    )
}
async fn package_center_css() -> Response {
    serve_asset("apps/package-center.css", "text/css; charset=utf-8")
}
async fn desktop_session_js() -> Response {
    serve_asset("session.js", "application/javascript; charset=utf-8")
}
async fn desktop_session_css() -> Response {
    serve_asset("session.css", "text/css; charset=utf-8")
}

#[cfg(test)]
fn router(policy: access::Policy) -> Router {
    router_with_apps(policy, apps::Store::system())
}
fn router_with_apps(policy: access::Policy, store: apps::Store) -> Router {
    Router::new()
        .merge(packages::router())
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/package-center.js", get(package_center_js))
        .route("/package-center.css", get(package_center_css))
        .route("/desktop-session.js", get(desktop_session_js))
        .route("/desktop-session.css", get(desktop_session_css))
        .route("/desktop-app-registry.js", get(app_registry_js))
        .route("/desktop-app-bridge.js", get(app_bridge_js))
        .route("/api/apps", get(apps::list))
        .route("/app-assets/{id}/{version}/{*path}", get(apps::asset))
        .route("/api/health", get(health))
        .route("/api/metrics", get(metrics))
        .route("/api/storage", get(storage_info))
        .route("/api/pools", get(pools_info))
        .route("/api/provisioning", get(provisioning_info))
        .layer(axum::Extension(std::sync::Arc::new(store)))
        .layer(axum::middleware::from_fn_with_state(policy, access::gate))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(mode) = std::env::args().nth(1) {
        if ["--lab-plan", "--lab-check"].contains(&mode.as_str()) {
            std::process::exit(lab::console(&mode).await);
        }
        eprintln!("Unknown mode; use --lab-plan or --lab-check, or no arguments for the API");
        std::process::exit(2);
    }
    let port: u16 = std::env::var("ZION_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080);
    let policy = access::Policy::runtime(port);
    let address = SocketAddr::from((policy.bind_ip(), port));

    let router = router_with_apps(policy, apps::Store::configured()?);

    let listener = TcpListener::bind(address).await?;
    println!("ZionOS Core {VERSION} listening on http://{address} (read-only, NO authentication)");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use tower::ServiceExt;

    fn request(
        path: &str,
        method: &str,
        peer: &str,
        host: &str,
        origin: Option<&str>,
    ) -> Request<Body> {
        let mut request = Request::builder()
            .uri(path)
            .method(method)
            .header("host", host)
            .extension(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        request.body(Body::empty()).unwrap()
    }
    #[tokio::test]
    async fn api_is_local_read_only_and_rejects_rebinding() {
        let local = router(access::Policy::new(false));
        let response = local
            .clone()
            .oneshot(request(
                "/api/health",
                "GET",
                "127.0.0.1:1234",
                "localhost:8080",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["readonly"], true);
        for route in [
            "health",
            "metrics",
            "storage",
            "pools",
            "provisioning",
            "apps",
        ] {
            let response = local
                .clone()
                .oneshot(request(
                    &format!("/api/{route}"),
                    "POST",
                    "127.0.0.1:1",
                    "localhost:8080",
                    None,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
        for (peer, host, origin) in [
            ("192.168.1.2:1", "localhost:8080", None),
            ("10.0.2.2:1", "localhost:8080", None),
            ("127.0.0.1:1", "attacker.example:8080", None),
            (
                "127.0.0.1:1",
                "localhost:8080",
                Some("http://attacker.example:8080"),
            ),
            ("127.0.0.1:1", "localhost:8080", Some("null")),
        ] {
            assert_eq!(
                local
                    .clone()
                    .oneshot(request("/api/health", "GET", peer, host, origin))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        let qemu = router(access::Policy::new(true));
        assert_eq!(
            qemu.oneshot(request(
                "/api/health",
                "GET",
                "10.0.2.2:1",
                "127.0.0.1:8080",
                None
            ))
            .await
            .unwrap()
            .status(),
            StatusCode::OK
        );
    }

    #[test]
    fn parses_memory_values() {
        let sample = "MemTotal:       2097152 kB\nMemAvailable:   1048576 kB\n";
        assert_eq!(parse_meminfo_kb(sample, "MemTotal"), Some(2_097_152));
        assert_eq!(parse_meminfo_kb(sample, "MemAvailable"), Some(1_048_576));
        assert_eq!(parse_meminfo_kb(sample, "Missing"), None);
    }

    #[test]
    fn calculates_cpu_usage() {
        assert_eq!(cpu_usage((100, 50), (200, 80)), Some(70.0));
        assert_eq!(cpu_usage((100, 50), (100, 50)), None);
    }

    #[test]
    fn parses_cpu_statistics() {
        assert_eq!(parse_cpu_times("cpu  2 3 4 5 6 7 8 9\n"), Some((44, 11)));
    }

    #[test]
    fn cpu_statistics_do_not_count_guest_time_twice() {
        assert_eq!(
            parse_cpu_times("cpu  2 3 4 5 6 7 8 9 2 1\n"),
            Some((44, 11))
        );
    }

    #[test]
    fn cpu_statistics_reject_invalid_or_overflowing_counters() {
        assert_eq!(parse_cpu_times("cpu 1 2 bad 4 5"), None);
        assert_eq!(parse_cpu_times("cpu0 1 2 3 4 5"), None);
        assert_eq!(parse_cpu_times("cpu 18446744073709551615 1 0 0 0"), None);
    }

    #[test]
    fn detects_cpu_model() {
        assert_eq!(
            cpu_model("processor : 0\nmodel name : Intel Example CPU\n"),
            "Intel Example CPU"
        );
    }
}
