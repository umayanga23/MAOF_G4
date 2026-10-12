//! One local writer; checkpoint replacement follows syncing the new file.
use domain::prototype::Workspace;
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
const MAX_CHECKPOINT_BYTES: usize = 100 * 1024 * 1024;
pub struct Store {
    directory: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("workspace.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|_| "This workspace is already open in another app process.".to_string())?;
        Ok(Self {
            directory: directory.to_owned(),
            _lock: lock,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn load(&self) -> Result<Option<Workspace>, String> {
        let path = self.directory.join("workspace.json");
        if !path.exists() {
            return Ok(None);
        }
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_CHECKPOINT_BYTES as u64 {
            return Err("Checkpoint is too large.".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let w: Workspace = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Checkpoint could not be read; original preserved: {e}"))?;
        if w.schema_version != 1 {
            return Err("Unsupported checkpoint version; original preserved.".into());
        }
        Ok(Some(w))
    }
    pub fn save(&self, workspace: &Workspace) -> Result<(), String> {
        self.save_with_limit(workspace, MAX_CHECKPOINT_BYTES)
    }
    fn save_with_limit(&self, workspace: &Workspace, limit: usize) -> Result<(), String> {
        // Bound encoding before touching the current checkpoint or temp file.
        struct Bytes {
            data: Vec<u8>,
            limit: usize,
        }
        impl Write for Bytes {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.data.len().saturating_add(bytes.len()) > self.limit {
                    return Err(std::io::Error::other(
                        "Checkpoint exceeds the supported byte limit.",
                    ));
                }
                self.data.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut bytes = Bytes {
            data: Vec::new(),
            limit,
        };
        serde_json::to_writer_pretty(&mut bytes, workspace).map_err(|e| e.to_string())?;
        let temp = self.directory.join("workspace.json.tmp");
        let mut file = File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(&bytes.data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(temp, self.directory.join("workspace.json")).map_err(|e| e.to_string())
    }
    pub fn export(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            || name.contains("..")
        {
            return Err("Invalid export name.".into());
        }
        let dir = self.directory.join("exports");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(name);
        let mut f = File::create(&path).map_err(|e| e.to_string())?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/storage-tests");
        root.join(format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn exclusive_lock_and_replacement_survive_reopening() {
        let directory = path();
        let store = Store::open(&directory).unwrap();
        assert!(Store::open(&directory).is_err());
        let mut w = Workspace {
            schema_version: 1,
            settings: Default::default(),
            agents: vec![],
            workflows: vec![],
            runs: vec![],
            documents: vec![],
        };
        store.save(&w).unwrap();
        w.settings.worker_slots = 4;
        store.save(&w).unwrap();
        drop(store);
        let reopened = Store::open(&directory).unwrap();
        assert_eq!(reopened.load().unwrap().unwrap().settings.worker_slots, 4);
        assert!(reopened.export("../escape.json", b"test").is_err());
    }
    #[test]
    fn malformed_checkpoint_is_not_silently_reset() {
        let directory = path();
        let store = Store::open(&directory).unwrap();
        let file = directory.join("workspace.json");
        fs::write(&file, "incomplete-json").unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "incomplete-json");
    }
    #[test]
    fn oversized_encoding_preserves_previous_checkpoint_and_never_opens_temp_file() {
        let directory = path();
        let store = Store::open(&directory).unwrap();
        let workspace = Workspace {
            schema_version: 1,
            settings: Default::default(),
            agents: vec![],
            workflows: vec![],
            runs: vec![],
            documents: vec![],
        };
        store.save(&workspace).unwrap();
        let before = fs::read(directory.join("workspace.json")).unwrap();
        assert!(store
            .save_with_limit(&workspace, 16)
            .unwrap_err()
            .contains("byte limit"));
        assert_eq!(fs::read(directory.join("workspace.json")).unwrap(), before);
        assert!(!directory.join("workspace.json.tmp").exists());
    }
}
