//! Single administrator, Argon2id; persistent hashed sessions/CSRF, no default credential.
use crate::{fail, hash, now, random, token, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
pub struct Auth {
    pub db: Connection,
}
fn argon() -> Result<Argon2<'static>> {
    Ok(Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, None)?,
    ))
}
fn password_ok(s: &str) -> bool {
    (12..=128).contains(&s.len()) && !s.contains('\0')
}
fn username_ok(s: &str) -> bool {
    (3..=32).contains(&s.len())
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
}
fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |n, (a, b)| n | (a ^ b)) == 0
}
impl Auth {
    pub fn open(path: &Path) -> Result<Self> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let m = f.metadata()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
        {
            return fail("unsafe authentication database");
        }
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
 CREATE TABLE IF NOT EXISTS administrator(id INTEGER PRIMARY KEY CHECK(id=1),hash TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS bootstrap(id INTEGER PRIMARY KEY CHECK(id=1),hash TEXT NOT NULL,expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS sessions(hash TEXT PRIMARY KEY,csrf TEXT NOT NULL,created INTEGER NOT NULL,last INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS throttle(id INTEGER PRIMARY KEY CHECK(id=1),fails INTEGER NOT NULL,until_time INTEGER NOT NULL);
 INSERT OR IGNORE INTO throttle VALUES(1,0,0);
 CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,kind TEXT NOT NULL,package TEXT NOT NULL,state TEXT NOT NULL,detail TEXT NOT NULL,time INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS previews(token TEXT PRIMARY KEY,session TEXT NOT NULL,digest TEXT NOT NULL,expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS audit(id INTEGER PRIMARY KEY,time INTEGER NOT NULL,kind TEXT NOT NULL,outcome TEXT NOT NULL);")?;
        let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return fail("corrupt authentication database");
        }
        let has_username = db
            .prepare("PRAGMA table_info(administrator)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .any(|s| s == "username");
        if !has_username {
            db.execute(
                "ALTER TABLE administrator ADD COLUMN username TEXT NOT NULL DEFAULT 'admin'",
                [],
            )?;
        }
        Ok(Self { db })
    }
    pub fn configured(&self) -> Result<bool> {
        Ok(self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM administrator)", [], |r| {
                r.get(0)
            })?)
    }
    pub fn bootstrap(&mut self, code: &str) -> Result<()> {
        if self.configured()? || !token(code) {
            return fail("administrator exists or invalid one-time bootstrap code");
        }
        self.db.execute("INSERT INTO bootstrap VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET hash=excluded.hash,expires=excluded.expires",params![hash(code.as_bytes()),now()?+600])?;
        Ok(())
    }
    pub fn username(&self) -> Result<String> {
        Ok(self
            .db
            .query_row("SELECT username FROM administrator WHERE id=1", [], |r| {
                r.get(0)
            })?)
    }
    /// First ZionOS owner enrollment. The local-only Core boundary and exact Origin
    /// gate are mandatory; only the dedicated Core peer can request this over IPC.
    pub fn enroll(&mut self, username: &str, password: &str) -> Result<()> {
        self.attempt()?;
        if self.configured()? || !username_ok(username) || !password_ok(password) {
            return fail("ZionOS is already configured or the username/password is invalid");
        }
        self.create_owner(username, password)
    }
    fn create_owner(&mut self, username: &str, password: &str) -> Result<()> {
        let salt = SaltString::encode_b64(&crate::entropy()?[..16])?;
        let hashed = argon()?
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO administrator(id,hash,username) VALUES(1,?1,?2)",
            params![hashed, username],
        )?;
        tx.execute("DELETE FROM bootstrap", [])?;
        tx.execute("UPDATE throttle SET fails=0,until_time=0", [])?;
        tx.commit()?;
        self.audit("setup", "ZionOS owner created")?;
        Ok(())
    }
    fn attempt(&self) -> Result<()> {
        let until: i64 =
            self.db
                .query_row("SELECT until_time FROM throttle WHERE id=1", [], |r| {
                    r.get(0)
                })?;
        if until > now()? {
            return fail("RATE_LIMITED");
        }
        Ok(())
    }
    fn failed(&self) -> Result<()> {
        self.db.execute("UPDATE throttle SET fails=fails+1,until_time=CASE WHEN fails>=4 THEN ?1 ELSE until_time END WHERE id=1",[now()?+60])?;
        self.audit("authentication", "denied")?;
        Ok(())
    }
    pub fn setup(&mut self, code: &str, password: &str) -> Result<()> {
        self.attempt()?;
        let boot: Option<(String, i64)> = self
            .db
            .query_row("SELECT hash,expires FROM bootstrap WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        if self.configured()?
            || !password_ok(password)
            || !token(code)
            || !boot.is_some_and(|(h, t)| {
                t >= now().unwrap_or(i64::MAX) && equal(&h, &hash(code.as_bytes()))
            })
        {
            self.failed()?;
            return fail("invalid setup code or password (12..128 bytes)");
        }
        self.create_owner("admin", password)
    }
    pub fn login(&mut self, password: &str) -> Result<(String, String)> {
        self.login_named("admin", password)
    }
    pub fn login_named(&mut self, username: &str, password: &str) -> Result<(String, String)> {
        self.attempt()?;
        let encoded: Option<String> = self
            .db
            .query_row("SELECT hash FROM administrator WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let valid = if let Some(ref encoded) = encoded {
            // Root-owned PHC corruption must not select unbounded memory/time parameters.
            if !encoded.starts_with("$argon2id$v=19$m=19456,t=2,p=1$") {
                return fail("corrupt password parameters");
            }
            username_ok(username)
                && equal(username, &self.username()?)
                && password_ok(password)
                && argon()?
                    .verify_password(password.as_bytes(), &PasswordHash::new(encoded)?)
                    .is_ok()
        } else {
            false
        };
        if !valid {
            self.failed()?;
            return fail("invalid administrator credentials");
        }
        let session = random()?;
        let csrf = random()?;
        let time = now()?;
        let tx = self.db.transaction()?;
        tx.execute(
            "DELETE FROM sessions WHERE created<?1 OR last<?2",
            params![time - 28800, time - 1200],
        )?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,?3)",
            params![hash(session.as_bytes()), csrf, time],
        )?;
        tx.execute("DELETE FROM sessions WHERE hash NOT IN (SELECT hash FROM sessions ORDER BY created DESC LIMIT 8)",[])?;
        tx.execute("UPDATE throttle SET fails=0,until_time=0", [])?;
        tx.commit()?;
        self.audit("login", "success")?;
        Ok((session, csrf))
    }
    pub fn authorize(&self, session: &str, csrf: Option<&str>) -> Result<()> {
        if !token(session) {
            return fail("UNAUTHENTICATED");
        }
        let row: Option<(String, i64, i64)> = self
            .db
            .query_row(
                "SELECT csrf,created,last FROM sessions WHERE hash=?1",
                [hash(session.as_bytes())],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((expected, created, last)) = row else {
            return fail("UNAUTHENTICATED");
        };
        let time = now()?;
        if time < created || time - created >= 28800 || time - last >= 1200 {
            return fail("SESSION_EXPIRED");
        }
        if let Some(csrf) = csrf {
            if !token(csrf) || !equal(&expected, csrf) {
                return fail("CSRF_DENIED");
            }
        }
        self.db.execute(
            "UPDATE sessions SET last=?1 WHERE hash=?2",
            params![time, hash(session.as_bytes())],
        )?;
        Ok(())
    }
    pub fn logout(&self, session: &str, csrf: &str) -> Result<()> {
        self.authorize(session, Some(csrf))?;
        self.db.execute(
            "DELETE FROM sessions WHERE hash=?1",
            [hash(session.as_bytes())],
        )?;
        self.audit("logout", "success")
    }
    pub fn csrf(&self, session: &str) -> Result<String> {
        self.authorize(session, None)?;
        Ok(self.db.query_row(
            "SELECT csrf FROM sessions WHERE hash=?1",
            [hash(session.as_bytes())],
            |r| r.get(0),
        )?)
    }
    pub fn audit(&self, kind: &str, outcome: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO audit(time,kind,outcome) VALUES(?1,?2,?3)",
            params![now()?, kind, outcome],
        )?;
        self.db.execute(
            "DELETE FROM audit WHERE id<(SELECT max(id)-4096 FROM audit)",
            [],
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_system_owner_named_login_and_no_reenrollment() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = Auth::open(&dir.path().join("auth")).unwrap();
        let pass = random().unwrap();
        for name in ["../root", "Root", "ab", "user\nname"] {
            assert!(a.enroll(name, &pass).is_err());
        }
        assert!(!a.configured().unwrap());
        a.enroll("owner", &pass).unwrap();
        assert_eq!(a.username().unwrap(), "owner");
        assert!(a.login_named("admin", &pass).is_err());
        let (s, c) = a.login_named("owner", &pass).unwrap();
        a.authorize(&s, Some(&c)).unwrap();
        assert!(a.enroll("replacement", &random().unwrap()).is_err());
        assert!(a.login_named("owner", &pass).is_ok());
    }
    #[test]
    fn legacy_account_keeps_password_and_sessions_as_system_admin() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("auth");
        let mut a = Auth::open(&file).unwrap();
        let pass = random().unwrap();
        a.enroll("admin", &pass).unwrap();
        let (s, c) = a.login(&pass).unwrap();
        let hash: String =
            a.db.query_row("SELECT hash FROM administrator", [], |r| r.get(0))
                .unwrap();
        a.db.execute("ALTER TABLE administrator DROP COLUMN username", [])
            .unwrap();
        drop(a);
        let mut a = Auth::open(&file).unwrap();
        assert_eq!(a.username().unwrap(), "admin");
        assert_eq!(
            a.db.query_row("SELECT hash FROM administrator", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            hash
        );
        a.authorize(&s, Some(&c)).unwrap();
        assert!(a.login_named("admin", &pass).is_ok());
        assert!(a.enroll("other", &pass).is_err());
    }
    #[test]
    fn setup_sessions_csrf_expiry_and_logout() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = Auth::open(&dir.path().join("auth.db")).unwrap();
        let code = random().unwrap();
        let pass = random().unwrap();
        assert!(!a.configured().unwrap());
        assert!(a.setup(&code, &pass).is_err());
        a.bootstrap(&code).unwrap();
        a.setup(&code, &pass).unwrap();
        assert!(a.bootstrap(&code).is_err());
        assert!(a.login("wrong-password-test").is_err());
        let (s, c) = a.login(&pass).unwrap();
        a.authorize(&s, Some(&c)).unwrap();
        assert!(a.authorize(&s, Some(&random().unwrap())).is_err());
        assert!(a.authorize("admin", None).is_err());
        a.logout(&s, &c).unwrap();
        assert!(a.authorize(&s, None).is_err());
        let (s, c) = a.login(&pass).unwrap();
        a.db.execute("UPDATE sessions SET last=0", []).unwrap();
        assert!(a.authorize(&s, Some(&c)).is_err());
        drop(a);
        let mut a = Auth::open(&dir.path().join("auth.db")).unwrap();
        assert!(a.configured().unwrap());
        assert!(a.login(&pass).is_ok());
    }
    #[test]
    fn throttles_and_refuses_links() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let mut a = Auth::open(&d.path().join("auth")).unwrap();
        for _ in 0..5 {
            assert!(a.login("incorrect-password").is_err())
        }
        assert!(a
            .login("incorrect-password")
            .unwrap_err()
            .to_string()
            .contains("RATE_LIMITED"));
        symlink(d.path().join("auth"), d.path().join("link")).unwrap();
        assert!(Auth::open(&d.path().join("link")).is_err());
    }
}
