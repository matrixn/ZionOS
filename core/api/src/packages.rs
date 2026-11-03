//! Authenticated HTTP facade. All privileged package effects occur in the root Unix broker.
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Request},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::io::AsyncWriteExt;
use zpk_control::{
    ipc::Client,
    protocol::{self, Action, Credentials, Reply, Request as Rpc},
};
#[derive(Debug)]
struct WebResponse(Box<Response>);
impl IntoResponse for WebResponse {
    fn into_response(self) -> Response {
        *self.0
    }
}
impl std::ops::Deref for WebResponse {
    type Target = Response;
    fn deref(&self) -> &Response {
        &self.0
    }
}
impl std::ops::DerefMut for WebResponse {
    fn deref_mut(&mut self) -> &mut Response {
        &mut self.0
    }
}
type WebResult<T> = std::result::Result<T, WebResponse>;
fn error(status: StatusCode, code: &str, message: &str) -> WebResponse {
    WebResponse(Box::new(
        (
            status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            Json(json!({"ok":false,"code":code,"message":message})),
        )
            .into_response(),
    ))
}
fn response(reply: Reply) -> WebResponse {
    let status = if reply.ok {
        StatusCode::OK
    } else {
        match reply.code.as_str() {
            "UNAUTHENTICATED" => StatusCode::UNAUTHORIZED,
            "CSRF_DENIED" => StatusCode::FORBIDDEN,
            "RATE_LIMITED" => StatusCode::TOO_MANY_REQUESTS,
            "BUSY" => StatusCode::CONFLICT,
            "INSUFFICIENT_STORAGE" => StatusCode::INSUFFICIENT_STORAGE,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        }
    };
    WebResponse(Box::new(
        (
            status,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            Json(reply),
        )
            .into_response(),
    ))
}
async fn rpc(request: Rpc, fd: Option<std::fs::File>) -> WebResult<Reply> {
    tokio::task::spawn_blocking(move || {
        Client::default().call(&request, fd.as_ref().map(AsRawFd::as_raw_fd))
    })
    .await
    .ok()
    .and_then(std::result::Result::ok)
    .ok_or_else(|| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "Persistent Package Center broker is unavailable; no RAM installation fallback",
        )
    })
}
fn session(headers: &HeaderMap) -> WebResult<String> {
    let mut found = None;
    for cookie in headers.get_all(header::COOKIE) {
        for part in cookie.to_str().unwrap_or_default().split(';') {
            if let Some(v) = part.trim().strip_prefix("zion_session=") {
                if found.is_some() || !zpk_control::token(v) {
                    return Err(error(
                        StatusCode::UNAUTHORIZED,
                        "UNAUTHENTICATED",
                        "Invalid session",
                    ));
                }
                found = Some(v.to_owned());
            }
        }
    }
    found.ok_or_else(|| {
        error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "Administrator login required",
        )
    })
}
fn origin(headers: &HeaderMap) -> WebResult<()> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if headers.get_all(header::ORIGIN).iter().count() != 1
        || headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
            != Some(format!("http://{host}").as_str())
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            "Exact local Origin and Host are required",
        ));
    }
    Ok(())
}
async fn authorized(headers: &HeaderMap, write: bool) -> WebResult<Credentials> {
    let session = session(headers)?;
    let reply = rpc(
        Rpc::Session {
            session: session.clone(),
        },
        None,
    )
    .await?;
    if !reply.ok {
        return Err(response(reply));
    }
    if reply.data["role"] != "administrator" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "Administrator authorization required",
        ));
    }
    let csrf = if write {
        origin(headers)?;
        let c = headers
            .get("x-zion-csrf")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if headers.get_all("x-zion-csrf").iter().count() != 1
            || !zpk_control::token(c)
            || reply.data["csrf"].as_str() != Some(c)
        {
            return Err(error(
                StatusCode::FORBIDDEN,
                "CSRF_DENIED",
                "A valid session-bound CSRF token is required",
            ));
        }
        c.to_owned()
    } else {
        String::new()
    };
    Ok(Credentials { session, csrf })
}
pub fn router() -> Router {
    Router::new()
        .route("/api/auth/status", get(status))
        .route("/api/auth/session", get(current))
        .route("/api/auth/setup", post(setup))
        .route("/api/auth/enroll", post(enroll))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/packages", get(installed))
        .route("/api/packages/catalog", get(catalog))
        .route("/api/packages/catalog/{id}/preview", post(catalog_preview))
        .route(
            "/api/packages/preview",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/packages/install", post(install))
        .route("/api/packages/action", post(action))
        .route("/api/packages/operations/{id}", get(progress))
        .route("/api/packages/logs/{id}", get(logs))
        .layer(DefaultBodyLimit::max(16 * 1024))
}
async fn status() -> WebResponse {
    match rpc(Rpc::Status {}, None).await {
        Ok(r) => response(r),
        Err(_) => {
            let mut detail=fs::read(protocol::STATUS).ok().filter(|b|b.len()<4096).and_then(|b|serde_json::from_slice::<Value>(&b).ok()).unwrap_or(json!({"available":false,"detail":"No configured persistent application volume"}));
            if detail["available"] == true {
                detail["available"] = json!(false);
                detail["detail"] =
                    json!("Package broker unavailable; persistent platform is degraded");
            }
            response(Reply::success(detail))
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Password {
    #[serde(default = "protocol::default_username")]
    username: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    username: String,
    password: String,
}
fn enrollment_origin(headers: &HeaderMap) -> WebResult<()> {
    origin(headers)?;
    let local = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.parse::<axum::http::uri::Authority>().ok())
        .is_some_and(|a| a.host() == "127.0.0.1" || a.host().eq_ignore_ascii_case("localhost"));
    if !local {
        return Err(error(
            StatusCode::FORBIDDEN,
            "LOCAL_SETUP_ONLY",
            "First ZionOS setup is available only through localhost",
        ));
    }
    Ok(())
}
async fn enroll(headers: HeaderMap, Json(body): Json<Enrollment>) -> WebResponse {
    if let Err(e) = enrollment_origin(&headers) {
        return e;
    }
    if body.username.len() > 32 || body.password.len() > 128 {
        return error(
            StatusCode::BAD_REQUEST,
            "INVALID_ENROLLMENT",
            "Invalid ZionOS username or password",
        );
    }
    match rpc(
        Rpc::Enroll {
            username: body.username,
            password: body.password,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    password: String,
    code: String,
}
async fn setup(headers: HeaderMap, Json(body): Json<Setup>) -> WebResponse {
    if let Err(e) = origin(&headers) {
        return e;
    }
    if body.password.len() > 128 || body.code.len() != 64 {
        return error(
            StatusCode::BAD_REQUEST,
            "INVALID_SETUP",
            "Invalid password or one-time code",
        );
    };
    match rpc(
        Rpc::Setup {
            code: body.code,
            password: body.password,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn login(headers: HeaderMap, Json(body): Json<Password>) -> WebResponse {
    if let Err(e) = origin(&headers) {
        return e;
    }
    if body.password.len() > 128 {
        return error(
            StatusCode::BAD_REQUEST,
            "INVALID_PASSWORD",
            "Password too long",
        );
    }
    match rpc(
        Rpc::Login {
            username: body.username,
            password: body.password,
        },
        None,
    )
    .await
    {
        Ok(mut r) => {
            if !r.ok {
                return response(r);
            }
            let Some(token) = r.data["session"]
                .as_str()
                .filter(|s| zpk_control::token(s))
                .map(str::to_owned)
            else {
                return error(
                    StatusCode::BAD_GATEWAY,
                    "INVALID_BROKER_REPLY",
                    "Invalid login response",
                );
            };
            r.data.as_object_mut().map(|v| v.remove("session"));
            let mut out = response(r);
            out.headers_mut().insert(
                header::SET_COOKIE,
                format!(
                    "zion_session={token}; Path=/api; HttpOnly; SameSite=Strict; Max-Age=28800"
                )
                .parse()
                .expect("validated token"),
            );
            out
        }
        Err(e) => e,
    }
}
async fn current(headers: HeaderMap) -> WebResponse {
    let s = match session(&headers) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match rpc(Rpc::Session { session: s }, None).await {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn logout(headers: HeaderMap) -> WebResponse {
    let a = match authorized(&headers, true).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(Rpc::Logout { auth: a }, None).await {
        Ok(r) => {
            let mut out = response(r);
            out.headers_mut().insert(
                header::SET_COOKIE,
                "zion_session=; Path=/api; HttpOnly; SameSite=Strict; Max-Age=0"
                    .parse()
                    .unwrap(),
            );
            out
        }
        Err(e) => e,
    }
}
async fn installed(headers: HeaderMap) -> WebResponse {
    let a = match authorized(&headers, false).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(Rpc::Installed { session: a.session }, None).await {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn catalog(headers: HeaderMap) -> WebResponse {
    let a = match authorized(&headers, false).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(Rpc::Catalog { session: a.session }, None).await {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn catalog_preview(headers: HeaderMap, Path(id): Path<String>) -> WebResponse {
    let a = match authorized(&headers, true).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(Rpc::CatalogPreview { auth: a, id }, None).await {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn progress(headers: HeaderMap, Path(id): Path<String>) -> WebResponse {
    let a = match authorized(&headers, false).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(
        Rpc::Progress {
            session: a.session,
            id,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn logs(headers: HeaderMap, Path(id): Path<String>) -> WebResponse {
    let a = match authorized(&headers, false).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(
        Rpc::Logs {
            session: a.session,
            id,
            stderr: false,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Install {
    receipt: String,
    upgrade: bool,
    permissions: Vec<zpk_schema::manifest::Permission>,
    trusted_local: bool,
    start: bool,
    autostart: bool,
    unix_health: bool,
}
async fn install(headers: HeaderMap, Json(b): Json<Install>) -> WebResponse {
    let a = match authorized(&headers, true).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(
        Rpc::Install {
            auth: a,
            receipt: b.receipt,
            upgrade: b.upgrade,
            permissions: b.permissions,
            trusted_local: b.trusted_local,
            start: b.start,
            autostart: b.autostart,
            unix_health: b.unix_health,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lifecycle {
    id: String,
    action: Action,
}
async fn action(headers: HeaderMap, Json(b): Json<Lifecycle>) -> WebResponse {
    let a = match authorized(&headers, true).await {
        Ok(a) => a,
        Err(e) => return e,
    };
    match rpc(
        Rpc::Lifecycle {
            auth: a,
            id: b.id,
            action: b.action,
        },
        None,
    )
    .await
    {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
static ACTIVE_UPLOADS: AtomicUsize = AtomicUsize::new(0);
struct UploadSlot;
impl UploadSlot {
    fn acquire(limit: usize) -> WebResult<Self> {
        let mut current = ACTIVE_UPLOADS.load(Ordering::SeqCst);
        loop {
            if current >= limit {
                return Err(error(
                    StatusCode::CONFLICT,
                    "BUSY",
                    "Upload concurrency limit reached",
                ));
            }
            match ACTIVE_UPLOADS.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(value) => current = value,
            }
        }
        Ok(Self)
    }
}
impl Drop for UploadSlot {
    fn drop(&mut self) {
        ACTIVE_UPLOADS.fetch_sub(1, Ordering::SeqCst);
    }
}
struct Upload(PathBuf);
impl Drop for Upload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
async fn upload(request: Request) -> WebResponse {
    match upload_inner(request).await {
        Ok(r) => response(r),
        Err(e) => e,
    }
}
async fn upload_inner(request: Request) -> WebResult<Reply> {
    let auth = authorized(request.headers(), true).await?;
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        != Some("application/octet-stream")
    {
        return Err(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "INVALID_UPLOAD",
            "Upload a raw ZPK file",
        ));
    }
    let policy = rpc(Rpc::Status {}, None).await?;
    if !policy.ok {
        return Ok(policy);
    }
    let limit = policy.data["max_upload"]
        .as_u64()
        .unwrap_or(protocol::MAX_UPLOAD)
        .min(protocol::MAX_UPLOAD);
    let reserve = policy.data["reserve_bytes"]
        .as_u64()
        .unwrap_or(protocol::RESERVE);
    let max_uploads = policy.data["max_uploads"].as_u64().unwrap_or(2).clamp(1, 2) as usize;
    let _slot = UploadSlot::acquire(max_uploads)?;
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|n| n > limit)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "UPLOAD_LIMIT",
            "Package exceeds configured upload size",
        ));
    }
    let root = std::path::Path::new(protocol::UPLOADS);
    let meta = fs::symlink_metadata(root).map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "Persistent upload staging unavailable",
        )
    })?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "Unsafe persistent upload directory",
        ));
    }
    let available = zpk_control::broker::free_bytes(root).map_err(|_| {
        error(
            StatusCode::INSUFFICIENT_STORAGE,
            "INSUFFICIENT_STORAGE",
            "Storage accounting unavailable",
        )
    })?;
    zpk_control::broker::capacity(available, limit, reserve).map_err(|_| {
        error(
            StatusCode::INSUFFICIENT_STORAGE,
            "INSUFFICIENT_STORAGE",
            "Persistent upload reserve unavailable",
        )
    })?;
    let guard = Upload(root.join(zpk_control::random().map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ENTROPY",
            "Secure randomness unavailable",
        )
    })?));
    let raw = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&guard.0)
        .map_err(|_| {
            error(
                StatusCode::INSUFFICIENT_STORAGE,
                "STAGING_FAILED",
                "Cannot create upload",
            )
        })?;
    let mut file = tokio::fs::File::from_std(raw);
    let mut body: Body = request.into_body();
    let mut size = 0u64;
    let started = std::time::Instant::now();
    while let Some(frame) = tokio::time::timeout(std::time::Duration::from_secs(30), body.frame())
        .await
        .map_err(|_| {
            error(
                StatusCode::REQUEST_TIMEOUT,
                "UPLOAD_TIMEOUT",
                "Upload stalled",
            )
        })?
    {
        let frame = frame.map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "UPLOAD_FAILED",
                "Invalid upload stream",
            )
        })?;
        if let Ok(bytes) = frame.into_data() {
            size = size.saturating_add(bytes.len() as u64);
            if size > limit || started.elapsed().as_secs() > 300 {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "UPLOAD_LIMIT",
                    "Package size/time limit exceeded",
                ));
            }
            zpk_control::broker::capacity(
                zpk_control::broker::free_bytes(root).unwrap_or(0),
                bytes.len() as u64,
                reserve,
            )
            .map_err(|_| {
                error(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "INSUFFICIENT_STORAGE",
                    "Upload storage reserve exhausted",
                )
            })?;
            file.write_all(&bytes).await.map_err(|_| {
                error(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "STAGING_FAILED",
                    "Persistent upload write failed",
                )
            })?;
        }
    }
    file.sync_all().await.map_err(|_| {
        error(
            StatusCode::INSUFFICIENT_STORAGE,
            "STAGING_FAILED",
            "Upload sync failed",
        )
    })?;
    drop(file);
    let fd = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&guard.0)
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "STAGING_FAILED",
                "Upload cannot be inspected",
            )
        })?;
    rpc(Rpc::Preview { auth }, Some(fd)).await
}
#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[test]
    fn configured_upload_concurrency_is_bounded_and_released() {
        let slot = UploadSlot::acquire(1).unwrap();
        assert!(UploadSlot::acquire(1).is_err());
        drop(slot);
        let _slot = UploadSlot::acquire(1).unwrap();
    }
    #[tokio::test]
    async fn no_session_cannot_use_package_routes() {
        for (path, method) in [
            ("/api/packages", "GET"),
            ("/api/packages/action", "POST"),
            ("/api/packages/preview", "POST"),
        ] {
            let r = router()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"id":"org.test.app","action":"start"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        }
    }
    #[test]
    fn exact_origins_and_cookie_ambiguity() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, "127.0.0.1:8080".parse().unwrap());
        assert!(origin(&h).is_err());
        h.insert(header::ORIGIN, "http://localhost:8080".parse().unwrap());
        assert!(origin(&h).is_err());
        h.insert(header::ORIGIN, "http://127.0.0.1:8080".parse().unwrap());
        origin(&h).unwrap();
        let t = "a".repeat(64);
        h.insert(
            header::COOKIE,
            format!("zion_session={t}; zion_session={t}")
                .parse()
                .unwrap(),
        );
        assert!(session(&h).is_err());
    }
    #[test]
    fn system_enrollment_requires_exact_localhost_origin() {
        let mut h = HeaderMap::new();
        for host in ["10.0.2.15:8080", "nas.example:8080"] {
            h.insert(header::HOST, host.parse().unwrap());
            h.insert(header::ORIGIN, format!("http://{host}").parse().unwrap());
            assert!(enrollment_origin(&h).is_err());
        }
        h.insert(header::HOST, "127.0.0.1:8080".parse().unwrap());
        h.insert(header::ORIGIN, "http://localhost:8080".parse().unwrap());
        assert!(enrollment_origin(&h).is_err());
        h.insert(header::ORIGIN, "http://127.0.0.1:8080".parse().unwrap());
        enrollment_origin(&h).unwrap();
    }
}
