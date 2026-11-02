use crate::{
    digest, fail,
    filesystem::Root,
    model::{Operation, Record},
    Result,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, fs, os::unix::fs::MetadataExt, path::Path};
pub struct Database {
    connection: Connection,
}
pub struct Journal {
    pub number: i64,
    pub state: String,
    pub operation: Operation,
}
fn decode<T: DeserializeOwned>(text: &str, hash: &str) -> Result<T> {
    if text.len() > 8 * 1024 * 1024 || digest(text.as_bytes()) != hash {
        return fail("corrupt registry/journal JSON digest");
    }
    Ok(serde_json::from_str(text)?)
}
fn encode<T: Serialize>(value: &T) -> Result<(String, String)> {
    let text = serde_json::to_string(value)?;
    if text.len() > 8 * 1024 * 1024 {
        return fail("registry record exceeds 8 MiB");
    }
    let hash = digest(text.as_bytes());
    Ok((text, hash))
}
impl Database {
    pub fn open(root: &Root, path: &Path) -> Result<Self> {
        if path.try_exists()? {
            root.check(path)?;
            let m = fs::metadata(path)?;
            if !m.is_file() || m.nlink() != 1 || m.mode() & 0o777 != 0o600 {
                return fail("registry must be a private regular file with one link");
            }
        } else {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)?
                .sync_all()?;
            crate::filesystem::sync(path.parent().ok_or("missing database parent")?)?;
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;")?;
        let page_size: i64 = connection.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let pages: i64 = connection.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let maximum = 64 * 1024 * 1024 / page_size;
        if pages > maximum {
            return fail(
                "registry exceeds the 64 MiB development budget; explicit maintenance required",
            );
        }
        connection.pragma_update(None, "max_page_count", maximum)?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            let count: i64 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )?;
            if count != 0 {
                return fail("unrecognized existing database; refusing initialization");
            }
            connection.execute_batch("BEGIN IMMEDIATE;
              CREATE TABLE packages(id TEXT PRIMARY KEY, record TEXT NOT NULL, hash TEXT NOT NULL);
              CREATE TABLE retained_data(id TEXT PRIMARY KEY, record TEXT NOT NULL, hash TEXT NOT NULL);
              CREATE TABLE operations(number INTEGER PRIMARY KEY AUTOINCREMENT, state TEXT NOT NULL CHECK(state IN ('PREPARED','STAGED','ACTIVATING','COMMITTED','FAILED','RECOVERY_REQUIRED')), record TEXT NOT NULL, hash TEXT NOT NULL, cleanup_needed INTEGER NOT NULL DEFAULT 1 CHECK(cleanup_needed IN (0,1)));
              CREATE TABLE diagnostics(number INTEGER PRIMARY KEY AUTOINCREMENT, operation INTEGER, time INTEGER NOT NULL DEFAULT(unixepoch()), message TEXT NOT NULL);
              PRAGMA user_version=1; COMMIT;")?;
        } else if version != 1 {
            return fail("unsupported package registry schema version");
        }
        let check: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return fail("SQLite integrity check failed");
        }
        Ok(Self { connection })
    }
    pub fn packages(&self, root: &Path) -> Result<BTreeMap<String, Record>> {
        self.records(root, false)
    }
    pub fn retained(&self, root: &Path) -> Result<BTreeMap<String, Record>> {
        self.records(root, true)
    }
    fn records(&self, root: &Path, retained: bool) -> Result<BTreeMap<String, Record>> {
        let mut q = self.connection.prepare(if retained {
            "SELECT id,record,hash FROM retained_data ORDER BY id"
        } else {
            "SELECT id,record,hash FROM packages ORDER BY id"
        })?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (id, text, hash) = row?;
            let record: Record = decode(&text, &hash)?;
            record.validate(root)?;
            if record.id != id || out.len() >= 1024 {
                return fail("registry identity/count mismatch");
            }
            out.insert(id, record);
        }
        Ok(out)
    }
    pub fn prepare(&self, op: &Operation) -> Result<i64> {
        let (text, hash) = encode(op)?;
        self.connection.execute(
            "INSERT INTO operations(state,record,hash) VALUES('PREPARED',?1,?2)",
            params![text, hash],
        )?;
        Ok(self.connection.last_insert_rowid())
    }
    pub fn phase(&self, n: i64, phase: &str) -> Result<()> {
        let changed=self.connection.execute("UPDATE operations SET state=?1 WHERE number=?2 AND ((?1='STAGED' AND state='PREPARED') OR (?1='ACTIVATING' AND state='STAGED') OR (?1 IN ('FAILED','RECOVERY_REQUIRED') AND state IN ('PREPARED','STAGED','ACTIVATING','RECOVERY_REQUIRED')))",params![phase,n])?;
        if changed != 1 {
            return fail("journal operation missing");
        }
        Ok(())
    }
    pub fn diagnostic(&self, n: i64, message: &str) -> Result<()> {
        let bounded: String = message.chars().take(4096).collect();
        self.connection.execute(
            "INSERT INTO diagnostics(operation,message) VALUES(?1,?2)",
            params![n, bounded],
        )?;
        Ok(())
    }
    pub fn pending(&self, root: &Path) -> Result<Vec<Journal>> {
        self.journals(root, false)
    }
    pub fn completed(&self, root: &Path) -> Result<Vec<Journal>> {
        self.journals(root, true)
    }
    fn journals(&self, root: &Path, completed: bool) -> Result<Vec<Journal>> {
        let mut q=self.connection.prepare(if completed{"SELECT number,state,record,hash FROM operations WHERE state='COMMITTED' AND cleanup_needed=1 ORDER BY number"}else{"SELECT number,state,record,hash FROM operations WHERE state NOT IN ('COMMITTED','FAILED') ORDER BY number"})?;
        let mut rows = q.query([])?;
        let mut out = vec![];
        while let Some(r) = rows.next()? {
            let text: String = r.get(2)?;
            let hash: String = r.get(3)?;
            let operation: Operation = decode(&text, &hash)?;
            operation.validate(root)?;
            out.push(Journal {
                number: r.get(0)?,
                state: r.get(1)?,
                operation,
            });
            if out.len() > 10000 {
                return fail("journal retention limit reached; explicit maintenance required");
            }
        }
        Ok(out)
    }
    pub fn commit(&mut self, n: i64, record: Option<&Record>, id: &str) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(record) = record {
            let (text, hash) = encode(record)?;
            tx.execute("INSERT INTO packages(id,record,hash) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET record=excluded.record,hash=excluded.hash",params![id,text,hash])?;
            tx.execute("DELETE FROM retained_data WHERE id=?1", [id])?;
        } else {
            tx.execute("INSERT INTO retained_data(id,record,hash) SELECT id,record,hash FROM packages WHERE id=?1 ON CONFLICT(id) DO UPDATE SET record=excluded.record,hash=excluded.hash",[id])?;
            tx.execute("DELETE FROM packages WHERE id=?1", [id])?;
        }
        if tx.execute(
            "UPDATE operations SET state='COMMITTED' WHERE number=?1 AND state='ACTIVATING'",
            [n],
        )? != 1
        {
            return fail("journal activation state missing");
        }
        tx.execute("INSERT INTO diagnostics(operation,message) VALUES(?1,'Operation committed; application data is not rolled back')",[n])?;
        tx.commit()?;
        Ok(())
    }
    pub fn service_state(&self, record: &Record) -> Result<()> {
        let (text, hash) = encode(record)?;
        self.connection.execute(
            "UPDATE packages SET record=?1,hash=?2 WHERE id=?3",
            params![text, hash, record.id],
        )?;
        Ok(())
    }
    pub fn cleanup_done(&self, n: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE operations SET cleanup_needed=0 WHERE number=?1 AND state='COMMITTED'",
            [n],
        )?;
        Ok(())
    }
    pub fn history(&self) -> Result<Vec<serde_json::Value>> {
        let mut q = self.connection.prepare(
            "SELECT operation,time,message FROM diagnostics ORDER BY number DESC LIMIT 100",
        )?;
        let rows=q.query_map([],|r|Ok(serde_json::json!({"operation":r.get::<_,i64>(0)?,"time":r.get::<_,i64>(1)?,"message":r.get::<_,String>(2)?})))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn operation_states(&self) -> Result<Vec<(i64, String)>> {
        let mut q = self
            .connection
            .prepare("SELECT number,state FROM operations ORDER BY number")?;
        let rows = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .query_row("SELECT id FROM packages WHERE id=?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .is_some())
    }
}
