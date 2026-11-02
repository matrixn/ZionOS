//! Read-only, local-development app discovery. Never opens Manager SQLite or launches processes.
use axum::{
    extract::{Extension, Path as Params},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};
use zpk_schema::desktop::{Entry, Registry, LIMIT, MAX_ASSET};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
#[derive(Clone)]
pub struct Store {
    root: PathBuf,
    owner: u32,
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
impl Store {
    pub fn system() -> Self {
        Self {
            root: "/".into(),
            owner: 0,
        }
    }
    pub fn configured() -> Result<Self> {
        if let Some(root) = std::env::var_os("ZION_APPS_TEST_ROOT") {
            if !cfg!(debug_assertions) || unsafe { libc::geteuid() } == 0 {
                return Err("app test roots require a non-root debug build".into());
            }
            let root = PathBuf::from(root);
            let m = fs::symlink_metadata(&root)?;
            if !root.is_absolute()
                || fs::canonicalize(&root)? != root
                || !m.is_dir()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o077 != 0
            {
                return Err("app test root must be a canonical private owned directory".into());
            }
            Ok(Self {
                root,
                owner: unsafe { libc::geteuid() },
            })
        } else {
            Ok(Self::system())
        }
    }
    fn secure(&self, path: &Path) -> Result<()> {
        let mut p = self.root.clone();
        self.metadata(&p)?;
        for c in path.strip_prefix(&self.root)?.components() {
            if !matches!(c, std::path::Component::Normal(_)) {
                return Err("noncanonical app path".into());
            }
            p.push(c);
            self.metadata(&p)?;
        }
        Ok(())
    }
    fn metadata(&self, p: &Path) -> Result<()> {
        let m = fs::symlink_metadata(p)?;
        if m.file_type().is_symlink() || m.uid() != self.owner || m.mode() & 0o022 != 0 {
            return Err("untrusted app path".into());
        }
        Ok(())
    }
    fn read(&self, path: &Path, max: usize) -> Result<Vec<u8>> {
        self.secure(path)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path)?;
        let m = file.metadata()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != self.owner
            || m.mode() & 0o022 != 0
            || m.len() > max as u64
        {
            return Err("unsafe app file".into());
        }
        let mut b = vec![];
        file.take(max as u64 + 1).read_to_end(&mut b)?;
        if b.len() > max {
            return Err("app read budget".into());
        }
        Ok(b)
    }
    fn registry(&self) -> Result<(String, Registry)> {
        let root = self.root.join("run/zionos/desktop");
        let before = self.read(&root.join("ready"), 64)?;
        let bytes = self.read(&root.join("registry.json"), LIMIT)?;
        let after = self.read(&root.join("ready"), 64)?;
        let revision = hash(&bytes);
        if before != after || before != revision.as_bytes() {
            return Err("app projection is updating".into());
        }
        Ok((revision, Registry::parse(&bytes)?))
    }
    fn version(&self, e: &Entry) -> Result<PathBuf> {
        let app = self.root.join("opt/zionos/apps").join(&e.package_id);
        self.secure(&app)?;
        let link = app.join("current");
        let m = fs::symlink_metadata(&link)?;
        if !m.file_type().is_symlink()
            || m.uid() != self.owner
            || fs::read_link(link)? != Path::new("versions").join(e.version.to_string())
        {
            return Err("app version is not active".into());
        }
        let path = app.join("versions").join(e.version.to_string());
        self.secure(&path)?;
        if hash(&self.read(&path.join("manifest.toml"), 65536)?) != e.manifest_sha256 {
            return Err("active app manifest changed".into());
        }
        Ok(path)
    }
    fn snapshot(&self) -> Snapshot {
        match self.registry() {
            Ok((revision, r)) => {
                let apps = r
                    .apps
                    .iter()
                    .filter(|e| self.version(e).is_ok())
                    .map(|e| PublicEntry {
                        id: e.id.clone(),
                        package_id: e.package_id.clone(),
                        version: e.version.to_string(),
                        desktop_id: e.desktop_id.clone(),
                        title: e.title.clone(),
                        categories: e.categories.clone(),
                        window: e.window.clone(),
                        multiple_instances: e.multiple_instances,
                        notifications: e.notifications,
                        entry_url: e.asset_url(&e.entry),
                        icon_url: e.asset_url(&e.icon),
                        revision: hash(serde_json::to_string(e).unwrap_or_default().as_bytes()),
                    })
                    .collect();
                Snapshot {
                    schema_version: 1,
                    status: "ready",
                    revision: Some(revision),
                    authentication: Authentication::development(),
                    apps,
                }
            }
            Err(_) => Snapshot {
                schema_version: 1,
                status: "unavailable",
                revision: None,
                authentication: Authentication::development(),
                apps: vec![],
            },
        }
    }
    fn asset(&self, id: &str, version: &str, path: &str) -> Result<(Vec<u8>, String, String)> {
        if !zpk_schema::manifest::package_id(id) || !zpk_schema::manifest::safe_path(path) {
            return Err("unsafe app asset reference".into());
        }
        let (revision, r) = self.registry()?;
        let e = r
            .apps
            .iter()
            .find(|e| {
                e.package_id == id
                    && e.version.to_string() == version
                    && e.assets.contains_key(path)
            })
            .ok_or("asset not registered")?;
        let info = &e.assets[path];
        let root = self.version(e)?;
        let bytes = self.read(&root.join(path), MAX_ASSET as usize)?;
        if bytes.len() as u64 != info.size || hash(&bytes) != info.sha256 {
            return Err("app asset hash mismatch".into());
        }
        if (info.mime.starts_with("text/") || info.mime.starts_with("application/javascript"))
            && std::str::from_utf8(&bytes)?.contains('\0')
        {
            return Err("invalid app text".into());
        }
        // Recheck both generation and current after reading: stale exports never expose old code.
        if self.registry()?.0 != revision || self.version(e)? != root {
            return Err("app changed while reading".into());
        }
        Ok((
            bytes,
            info.mime.clone(),
            format!("/app-assets/{id}/{version}/"),
        ))
    }
}
#[derive(Serialize)]
pub struct Authentication {
    mode: &'static str,
    authenticated: bool,
    required_for_remote: bool,
}
impl Authentication {
    fn development() -> Self {
        Self {
            mode: "local-development",
            authenticated: false,
            required_for_remote: true,
        }
    }
}
#[derive(Serialize)]
pub struct PublicEntry {
    id: String,
    package_id: String,
    version: String,
    desktop_id: String,
    title: String,
    categories: Vec<String>,
    window: zpk_schema::manifest::Window,
    multiple_instances: bool,
    notifications: bool,
    entry_url: String,
    icon_url: String,
    revision: String,
}
#[derive(Serialize)]
pub struct Snapshot {
    schema_version: u32,
    status: &'static str,
    revision: Option<String>,
    authentication: Authentication,
    apps: Vec<PublicEntry>,
}
pub async fn list(Extension(store): Extension<Arc<Store>>) -> Response {
    match tokio::task::spawn_blocking(move || store.snapshot()).await {
        Ok(snapshot) => (
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            Json(snapshot),
        )
            .into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "app registry unavailable").into_response(),
    }
}
pub async fn asset(
    Extension(store): Extension<Arc<Store>>,
    Params((id, version, path)): Params<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(authority) = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<axum::http::uri::Authority>().ok())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let origin = format!("http://{authority}");
    match tokio::task::spawn_blocking(move || store.asset(&id, &version, &path)).await {
        Ok(Ok((bytes, mime, prefix))) => {
            let csp=format!("default-src 'none'; sandbox allow-scripts; script-src 'unsafe-inline' {origin}{prefix}; style-src 'unsafe-inline' {origin}{prefix}; img-src {origin}{prefix} data:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'; worker-src 'none'; frame-ancestors 'self'");
            ([(header::CONTENT_TYPE,mime),
              (header::CACHE_CONTROL,"no-store".into()),
              (header::X_CONTENT_TYPE_OPTIONS,"nosniff".into()),
              (header::CONTENT_SECURITY_POLICY,csp),
              (header::REFERRER_POLICY,"no-referrer".into()),
              (header::ACCESS_CONTROL_ALLOW_ORIGIN,"*".into()),
              (axum::http::HeaderName::from_static("permissions-policy"),"camera=(), microphone=(), geolocation=(), usb=(), serial=(), hid=(), payment=(), fullscreen=(), clipboard-read=(), clipboard-write=()".into())],bytes).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "registered app asset unavailable").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, extract::ConnectInfo, http::Request};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tower::ServiceExt;
    fn fixture() -> (tempfile::TempDir, Store, Entry) {
        let root = tempfile::tempdir().unwrap();
        let store = Store {
            root: root.path().into(),
            owner: unsafe { libc::geteuid() },
        };
        let source = include_str!("../../../examples/zion-hello/zpk.toml")
            .split("[forge]")
            .next()
            .unwrap();
        let manifest = zpk_schema::Manifest::parse(source).unwrap();
        let app = root
            .path()
            .join("opt/zionos/apps")
            .join(&manifest.package.id);
        let version = app.join("versions/1.0.0");
        fs::create_dir_all(version.join("ui")).unwrap();
        fs::create_dir_all(version.join("assets")).unwrap();
        fs::write(version.join("manifest.toml"), source).unwrap();
        symlink("versions/1.0.0", app.join("current")).unwrap();
        let mut assets = std::collections::BTreeMap::new();
        for (path, bytes, mime) in [
            (
                "ui/index.html",
                b"<html><body>fixture</body></html>".as_slice(),
                "text/html; charset=utf-8",
            ),
            (
                "assets/icon.png",
                include_bytes!("../../../examples/zion-hello/assets/icon.png").as_slice(),
                "image/png",
            ),
            (
                "assets/test.js",
                b"window.fixture = true;".as_slice(),
                "application/javascript; charset=utf-8",
            ),
        ] {
            fs::write(version.join(path), bytes).unwrap();
            assets.insert(
                path.into(),
                zpk_schema::desktop::Asset {
                    size: bytes.len() as u64,
                    sha256: hash(bytes),
                    mime: mime.into(),
                },
            );
        }
        let entry = Entry::from_manifest(&manifest, assets, hash(source.as_bytes()))
            .unwrap()
            .remove(0);
        publish(&store, &entry);
        (root, store, entry)
    }
    fn publish(store: &Store, entry: &Entry) {
        let dir = store.root.join("run/zionos/desktop");
        fs::create_dir_all(&dir).unwrap();
        let data = serde_json::to_vec(&Registry {
            schema_version: 1,
            apps: vec![entry.clone()],
        })
        .unwrap();
        fs::write(dir.join("registry.json"), &data).unwrap();
        fs::write(dir.join("ready"), hash(&data)).unwrap();
    }
    fn request(path: &str, method: &str, origin: Option<&str>) -> Request<Body> {
        let mut b = Request::builder()
            .uri(path)
            .method(method)
            .header("host", "localhost:8080")
            .extension(ConnectInfo(
                "127.0.0.1:10000".parse::<std::net::SocketAddr>().unwrap(),
            ));
        if let Some(origin) = origin {
            b = b.header("origin", origin);
        }
        b.body(Body::empty()).unwrap()
    }
    #[tokio::test]
    async fn discovery_is_read_only_and_explicit_about_authentication() {
        let (_root, store, entry) = fixture();
        let app = crate::router_with_apps(crate::access::Policy::new(false), store);
        let response = app
            .clone()
            .oneshot(request("/api/apps", "GET", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 32768)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["apps"][0]["id"], entry.id);
        assert_eq!(value["authentication"]["authenticated"], false);
        assert_eq!(value["authentication"]["required_for_remote"], true);
        assert!(!String::from_utf8_lossy(&bytes).contains("/var/lib/"));
        assert_eq!(
            app.clone()
                .oneshot(request("/api/apps", "POST", None))
                .await
                .unwrap()
                .status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            app.oneshot(request("/api/apps", "GET", Some("null")))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn asset_headers_and_opaque_origin_exception_do_not_open_the_api() {
        let (_root, store, entry) = fixture();
        let app = crate::router_with_apps(crate::access::Policy::new(false), store);
        let url = entry.asset_url("assets/test.js");
        let response = app
            .clone()
            .oneshot(request(&url, "GET", Some("null")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        let csp = response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("sandbox allow-scripts"));
        assert!(!csp.contains("allow-same-origin"));
        assert!(csp.contains("connect-src 'none'"));
        assert!(csp.contains("http://localhost:8080/app-assets/"));
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        assert_eq!(
            app.clone()
                .oneshot(request(&url, "GET", Some("https://attacker.example")))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        for path in [
            entry.asset_url("payload/bin/zion-hello"),
            entry.asset_url("manifest.toml"),
            entry.asset_url("LICENSE"),
            entry.asset_url("%2e%2e%2fetc%2fpasswd"),
            entry.asset_url("%2fetc%2fpasswd"),
        ] {
            assert_ne!(
                app.clone()
                    .oneshot(request(&path, "GET", None))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
    }
    #[test]
    fn inactive_versions_and_changed_manifest_are_hidden() {
        let (root, store, entry) = fixture();
        assert_eq!(store.snapshot().apps.len(), 1);
        let current = root
            .path()
            .join("opt/zionos/apps")
            .join(&entry.package_id)
            .join("current");
        fs::remove_file(&current).unwrap();
        assert!(store.snapshot().apps.is_empty());
        assert!(store
            .asset(&entry.package_id, "1.0.0", "ui/index.html")
            .is_err());
        symlink("versions/1.0.0", &current).unwrap();
        fs::write(
            root.path()
                .join("opt/zionos/apps")
                .join(&entry.package_id)
                .join("versions/1.0.0/manifest.toml"),
            "changed",
        )
        .unwrap();
        assert!(store.snapshot().apps.is_empty());
    }
    #[test]
    fn invalid_projection_hash_unknown_schema_and_traversal_fail_closed() {
        let (root, store, mut entry) = fixture();
        let dir = root.path().join("run/zionos/desktop");
        fs::write(dir.join("ready"), "0".repeat(64)).unwrap();
        assert!(store.snapshot().apps.is_empty());
        let data = b"{\"schema_version\":2,\"apps\":[]}";
        fs::write(dir.join("registry.json"), data).unwrap();
        fs::write(dir.join("ready"), hash(data)).unwrap();
        assert!(store.registry().is_err());
        entry.entry = "../private".into();
        publish(&store, &entry);
        assert!(store.registry().is_err());
    }
    #[test]
    fn asset_symlink_hardlink_mode_and_hash_changes_are_refused() {
        let (root, store, entry) = fixture();
        let version = root
            .path()
            .join("opt/zionos/apps")
            .join(&entry.package_id)
            .join("versions/1.0.0");
        let path = version.join("assets/test.js");
        fs::write(&path, "changed").unwrap();
        assert!(store
            .asset(&entry.package_id, "1.0.0", "assets/test.js")
            .is_err());
        fs::remove_file(&path).unwrap();
        symlink("/etc/passwd", &path).unwrap();
        assert!(store
            .asset(&entry.package_id, "1.0.0", "assets/test.js")
            .is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"window.fixture = true;").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(store
            .asset(&entry.package_id, "1.0.0", "assets/test.js")
            .is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::hard_link(&path, version.join("assets/hard.js")).unwrap();
        assert!(store
            .asset(&entry.package_id, "1.0.0", "assets/test.js")
            .is_err());
    }
}
