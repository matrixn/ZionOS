use serde::{Deserialize, Serialize};
use zpk_schema::manifest::Permission;
pub const SOCKET: &str = "/run/zionos/control/broker.sock";
pub const UPLOADS: &str = "/var/lib/zionos/uploads";
pub const STATE: &str = "/var/lib/zionos/platform";
pub const STATUS: &str = "/run/zionos/platform.json";
pub const MAX_UPLOAD: u64 = 64 * 1024 * 1024;
pub const RESERVE: u64 = 64 * 1024 * 1024;
pub const FRAME: usize = 16 * 1024;
pub const RESPONSE: usize = 2 * 1024 * 1024;
pub const CATALOG_INDEX_LIMIT: u64 = 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub session: String,
    pub csrf: String,
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Start,
    Stop,
    Restart,
    Rollback,
    Uninstall,
    Verify,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Rollback => "rollback",
            Self::Uninstall => "uninstall",
            Self::Verify => "verify",
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Status {},
    Setup {
        code: String,
        password: String,
    },
    Login {
        password: String,
    },
    Session {
        session: String,
    },
    Logout {
        auth: Credentials,
    },
    Installed {
        session: String,
    },
    Logs {
        session: String,
        id: String,
        stderr: bool,
    },
    Catalog {
        session: String,
    },
    Preview {
        auth: Credentials,
    },
    CatalogPreview {
        auth: Credentials,
        id: String,
    },
    Install {
        auth: Credentials,
        receipt: String,
        upgrade: bool,
        permissions: Vec<Permission>,
        trusted_local: bool,
        start: bool,
        autostart: bool,
        unix_health: bool,
    },
    Lifecycle {
        auth: Credentials,
        id: String,
        action: Action,
    },
    Progress {
        session: String,
        id: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub ok: bool,
    pub code: String,
    pub message: String,
    pub data: serde_json::Value,
}
impl Reply {
    pub fn success(data: impl Serialize) -> Self {
        Self {
            ok: true,
            code: "OK".into(),
            message: String::new(),
            data: serde_json::to_value(data).unwrap_or_default(),
        }
    }
    pub fn error(code: &str, message: &str) -> Self {
        Self {
            ok: false,
            code: code.into(),
            message: message
                .chars()
                .take(1024)
                .filter(|c| !c.is_control())
                .collect(),
            data: serde_json::Value::Null,
        }
    }
}
