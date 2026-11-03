//! One bounded application-owned setting. No browser endpoint, shell or arbitrary paths.
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
pub fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 160 && !s.chars().any(char::is_control)
}
pub fn read(path: &Path) -> io::Result<String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x20000 | 0x800) // Linux O_NOFOLLOW | O_NONBLOCK
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file() || m.nlink() != 1 || m.len() > 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsafe settings file",
        ));
    }
    let mut value = String::new();
    file.take(1025).read_to_string(&mut value)?;
    if value.len() > 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "settings limit"));
    }
    Ok(value)
}
pub fn atomic(path: &Path, value: &str) -> io::Result<()> {
    let temporary = path.with_extension(format!("pending-{}", std::process::id()));
    // A stale file is not silently overwritten. Applications may need explicit local recovery.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(value.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(
            path.parent()
                .ok_or_else(|| io::Error::other("settings parent"))?,
        )?
        .sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_only_bounded_single_line_values() {
        for bad in ["", "   ", "a\nb", "a\0b", &"a".repeat(161)] {
            assert!(!valid(bad));
        }
        assert!(valid("Hello, ZionOS"));
        assert!(valid("Salut, România"));
    }
    #[test]
    fn persists_atomically_and_refuses_links_oversize_and_stale_temporary_files() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = std::env::temp_dir().join(format!(
            "zion-hello-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("greeting.txt");
        atomic(&path, "before").unwrap();
        assert_eq!(read(&path).unwrap(), "before");
        atomic(&path, "after").unwrap();
        assert_eq!(read(&path).unwrap(), "after");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = dir.join("symlink");
        symlink(&path, &link).unwrap();
        assert!(read(&link).is_err());
        let hard = dir.join("hardlink");
        fs::hard_link(&path, &hard).unwrap();
        assert!(read(&path).is_err());
        fs::remove_file(hard).unwrap();
        let pending = path.with_extension(format!("pending-{}", std::process::id()));
        fs::write(&pending, "stale").unwrap();
        assert!(atomic(&path, "lost").is_err());
        assert_eq!(read(&path).unwrap(), "after");
        assert_eq!(fs::read_to_string(pending).unwrap(), "stale");
        fs::write(&path, "x".repeat(1025)).unwrap();
        assert!(read(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
