use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

const BUDGET: u64 = 64 * 1024 * 1024;
const ENTRIES: usize = 512;
const MAX_FILE: u64 = 8 * 1024 * 1024;

pub struct Disk {
    root: Option<PathBuf>,
    writer: Mutex<()>,
}
impl Disk {
    pub fn shared() -> Arc<Self> {
        static CACHE: OnceLock<Arc<Disk>> = OnceLock::new();
        CACHE.get_or_init(|| Arc::new(Disk::new())).clone()
    }
    pub fn new() -> Self {
        let root = directories::ProjectDirs::from("app", "larpcord", "Larpcord")
            .map(|d| d.cache_dir().join("media-v1"));
        let cache = Self {
            root,
            writer: Mutex::new(()),
        };
        cache.prune();
        cache
    }
    fn path(&self, url: &str) -> Option<PathBuf> {
        let key = url.split('#').next()?;
        Some(
            self.root
                .as_ref()?
                .join(format!("{:x}.image", Sha256::digest(key.as_bytes()))),
        )
    }
    pub fn read(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.path(url)?;
        let metadata = fs::metadata(&path).ok()?;
        if metadata.len() > MAX_FILE
            || metadata.modified().ok()?.elapsed().ok()? > Duration::from_secs(30 * 86400)
        {
            let _ = fs::remove_file(path);
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        // touching a hit makes eviction follow use across restarts, without a separate metadata file.
        if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(bytes)
    }
    pub fn remove(&self, url: &str) {
        if let Some(path) = self.path(url) {
            let _ = fs::remove_file(path);
        }
    }
    pub fn write(&self, url: &str, bytes: &[u8]) {
        if bytes.is_empty() || bytes.len() as u64 > MAX_FILE {
            return;
        }
        let Some(path) = self.path(url) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(_guard) = self.writer.lock() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(mut file) = tempfile::NamedTempFile::new_in(parent)
            && file.write_all(bytes).is_ok()
        {
            let _ = file.persist(path);
        }
        self.prune();
    }
    fn prune(&self) {
        let Some(root) = &self.root else {
            return;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        let mut files = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()?.to_str()? != "image" {
                    return None;
                }
                let m = entry.metadata().ok()?;
                Some((
                    m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    m.len(),
                    path,
                ))
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|f| f.0);
        let mut total = files.iter().map(|f| f.1).sum::<u64>();
        let mut count = files.len();
        for (modified, bytes, path) in files {
            if total <= BUDGET
                && count <= ENTRIES
                && modified.elapsed().unwrap_or_default() <= Duration::from_secs(30 * 86400)
            {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(bytes);
                count -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_and_still_share_downloads_and_the_cache_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk {
            root: Some(dir.path().into()),
            writer: Mutex::new(()),
        };
        let url = "https://cdn.discordapp.com/icons/1/abc.png";
        disk.write(url, b"encoded image");
        let reopened = Disk {
            root: Some(dir.path().into()),
            writer: Mutex::new(()),
        };
        assert_eq!(
            reopened.read(&format!("{url}#motion")).unwrap(),
            b"encoded image"
        );
        reopened.remove(url);
        assert!(reopened.read(url).is_none());
    }
    #[test]
    fn compressed_files_stay_inside_the_disk_allowance() {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk {
            root: Some(dir.path().into()),
            writer: Mutex::new(()),
        };
        let bytes = vec![0; 8 * 1024 * 1024];
        for n in 0..10 {
            disk.write(&format!("https://cdn.discordapp.com/{n}.png"), &bytes);
        }
        let total: u64 = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|f| f.metadata().unwrap().len())
            .sum();
        assert!(total <= BUDGET);
    }

    #[test]
    fn a_cache_hit_refreshes_file_age_on_every_platform() {
        let dir = tempfile::tempdir().unwrap();
        let disk = Disk {
            root: Some(dir.path().into()),
            writer: Mutex::new(()),
        };
        let url = "https://cdn.discordapp.com/icons/1/abc.webp";
        disk.write(url, b"image");
        let path = disk.path(url).unwrap();
        let before = SystemTime::now() - Duration::from_secs(2 * 86400);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(before)
            .unwrap();
        assert!(disk.read(url).is_some());
        assert!(
            fs::metadata(path).unwrap().modified().unwrap() > before + Duration::from_secs(86400)
        );
    }
}
