use crate::{
    files::{self, Listing},
    security,
    ssh::Connection,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, EventTarget};
use tokio::sync::Notify;

const MAX_DIRECTORIES: usize = 2000;
const MAX_ENTRIES: usize = 100_000;
pub const MAX_DEPTH: u64 = 16;
const REFRESH: Duration = Duration::from_secs(30);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub enabled: bool,
    pub scanning: bool,
    pub root: String,
    pub depth: u8,
    pub folders: usize,
    pub entries: usize,
    pub updated_at: u64,
    pub errors: Vec<String>,
    pub partial: bool,
}
#[derive(Default)]
struct Inner {
    generation: u64,
    revision: u64,
    listings: HashMap<String, Listing>,
    status: ScanStatus,
}
#[derive(Default)]
pub struct DirectoryCache {
    inner: Mutex<Inner>,
    wake: Notify,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn normalize(path: &str) -> String {
    let p = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/");
    format!("/{p}")
}
pub fn contains(root: &str, path: &str, depth: u8) -> bool {
    let root = normalize(root);
    let path = normalize(path);
    if root == path {
        return true;
    }
    let prefix = if root == "/" {
        "/".into()
    } else {
        format!("{root}/")
    };
    path.strip_prefix(&prefix)
        .is_some_and(|suffix| suffix.split('/').count() <= usize::from(depth))
}
fn subtree(root: &str, path: &str) -> bool {
    root == "/" || path == root || path.starts_with(&format!("{root}/"))
}
fn parent(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(p, _)| if p.is_empty() { "/" } else { p })
        .unwrap_or("/")
}
impl DirectoryCache {
    pub fn status(&self) -> ScanStatus {
        self.inner.lock().unwrap().status.clone()
    }
    pub fn get(&self, path: &str) -> Option<Listing> {
        let inner = self.inner.lock().unwrap();
        if !inner.status.enabled {
            return None;
        }
        inner
            .listings
            .get(&normalize(path))
            .cloned()
            .map(|mut listing| {
                listing.cached = true;
                listing
            })
    }
    pub fn remember(&self, listing: Listing) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let total: usize = inner.listings.values().map(|l| l.entries.len()).sum();
        let old = inner
            .listings
            .get(&listing.path)
            .map_or(0, |l| l.entries.len());
        if inner.status.enabled
            && contains(&inner.status.root, &listing.path, inner.status.depth)
            && (inner.listings.contains_key(&listing.path)
                || inner.listings.len() < MAX_DIRECTORIES)
            && total - old + listing.entries.len() <= MAX_ENTRIES
        {
            let changed = inner
                .listings
                .get(&listing.path)
                .is_none_or(|old| old.entries != listing.entries);
            inner.listings.insert(listing.path.clone(), listing);
            return changed;
        }
        false
    }
    pub fn invalidate(&self, paths: &[&str]) {
        let paths: Vec<_> = paths.iter().map(|p| normalize(p)).collect();
        let mut inner = self.inner.lock().unwrap();
        inner.revision += 1;
        inner.listings.retain(|cached, _| {
            !paths
                .iter()
                .any(|p| subtree(p, cached) || parent(p) == cached)
        });
        drop(inner);
        self.wake.notify_waiters();
    }
    pub fn stop(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        inner.listings.clear();
        inner.status = ScanStatus::default();
        drop(inner);
        self.wake.notify_waiters();
    }
    fn current(&self, generation: u64) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.generation == generation && inner.status.enabled
    }
    pub async fn start(
        conn: Arc<Connection>,
        app: tauri::AppHandle,
        root: &str,
        depth: u64,
    ) -> Result<ScanStatus> {
        security::path(root)?;
        if depth > MAX_DEPTH {
            bail!("Scan depth must be between 0 and {MAX_DEPTH}. Depth 0 caches only the current folder.")
        }
        let root = conn.sftp.canonicalize(root).await?;
        if !conn.sftp.metadata(&root).await?.is_dir() {
            bail!("Choose a remote directory to scan")
        }
        let generation = {
            let mut inner = conn.directory_cache.inner.lock().unwrap();
            inner.generation += 1;
            inner.listings.clear();
            inner.status = ScanStatus {
                enabled: true,
                scanning: true,
                root: root.clone(),
                depth: depth as u8,
                ..Default::default()
            };
            inner.generation
        };
        let status = conn.directory_cache.status();
        conn.directory_cache.wake.notify_waiters();
        let _ = app.emit_to(EventTarget::webview("main"), "directory-scan", &status);
        tokio::spawn(async move {
            loop {
                if !conn.directory_cache.current(generation) || conn.handle.is_closed() {
                    break;
                }
                let revision = conn.directory_cache.inner.lock().unwrap().revision;
                conn.directory_cache
                    .pass(&conn, &app, generation, revision, &root, depth as u8)
                    .await;
                if !conn.directory_cache.current(generation) {
                    break;
                }
                if conn.directory_cache.inner.lock().unwrap().revision != revision {
                    continue;
                }
                tokio::select! {_=tokio::time::sleep(REFRESH)=>{},_=conn.directory_cache.wake.notified()=>{}}
            }
        });
        Ok(status)
    }
    async fn pass(
        &self,
        conn: &Connection,
        app: &tauri::AppHandle,
        generation: u64,
        revision: u64,
        root: &str,
        depth: u8,
    ) {
        let mut progress = ScanStatus {
            enabled: true,
            scanning: true,
            root: root.into(),
            depth,
            ..Default::default()
        };
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.generation != generation {
                return;
            }
            inner.status.scanning = true;
        }
        let _ = app.emit_to(
            EventTarget::webview("main"),
            "directory-scan",
            self.status(),
        );
        let mut queue = VecDeque::from([(root.to_string(), 0u8)]);
        let mut visited = HashSet::new();
        let mut attempted = 0;
        let started = Instant::now();
        let mut last_event = Instant::now();
        while let Some((path, level)) = queue.pop_front() {
            if !self.current(generation) {
                return;
            }
            if attempted >= MAX_DIRECTORIES
                || progress.entries >= MAX_ENTRIES
                || started.elapsed() > Duration::from_secs(120)
            {
                progress.partial = true;
                if progress.errors.len() < 5 {
                    progress.errors.push("Scan limit reached (2,000 folders, 100,000 entries or two minutes); choose a smaller depth.".into());
                }
                break;
            }
            attempted += 1;
            match files::read_listing(conn, &path).await {
                Ok(listing) => {
                    if progress.entries + listing.entries.len() > MAX_ENTRIES {
                        progress.partial = true;
                        if progress.errors.len() < 5 {
                            progress.errors.push(
                                "Entry limit reached; choose a smaller directory tree.".into(),
                            );
                        }
                        break;
                    }
                    // Never walk symlinks or paths outside the selected root.
                    if !contains(root, &listing.path, depth)
                        || !visited.insert(listing.path.clone())
                    {
                        continue;
                    }
                    if level < depth {
                        for e in &listing.entries {
                            if e.is_dir && !e.is_link {
                                queue.push_back((e.path.clone(), level + 1));
                            }
                        }
                    }
                    progress.folders += 1;
                    progress.entries += listing.entries.len();
                    let changed = {
                        let mut inner = self.inner.lock().unwrap();
                        if inner.generation != generation || inner.revision != revision {
                            return;
                        }
                        let old = inner
                            .listings
                            .get(&listing.path)
                            .map_or(0, |l| l.entries.len());
                        let total: usize = inner.listings.values().map(|l| l.entries.len()).sum();
                        if total - old + listing.entries.len() > MAX_ENTRIES
                            || inner.listings.len() >= MAX_DIRECTORIES
                        {
                            inner.listings.retain(|p, _| visited.contains(p));
                        }
                        let changed = inner
                            .listings
                            .get(&listing.path)
                            .is_none_or(|old| old.entries != listing.entries);
                        inner.listings.insert(listing.path.clone(), listing.clone());
                        changed
                    };
                    if changed {
                        let _ = app.emit_to(
                            EventTarget::webview("main"),
                            "directory-cache-updated",
                            serde_json::json!({"path":listing.path}),
                        );
                    }
                }
                Err(e) => {
                    if progress.errors.len() < 5 {
                        progress.errors.push(format!("{path}: {e}"));
                    }
                    progress.partial = true;
                }
            }
            if last_event.elapsed() > Duration::from_millis(200) {
                let mut inner = self.inner.lock().unwrap();
                if inner.generation != generation {
                    return;
                }
                inner.status = progress.clone();
                drop(inner);
                let _ = app.emit_to(EventTarget::webview("main"), "directory-scan", &progress);
                last_event = Instant::now();
            }
        }
        progress.scanning = false;
        progress.updated_at = now();
        let mut removed = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.generation != generation || inner.revision != revision {
                return;
            }
            // Only prune an exhaustive pass: unreadable or bounded scans retain older snapshots.
            if !progress.partial {
                removed = inner
                    .listings
                    .keys()
                    .filter(|p| !visited.contains(*p))
                    .cloned()
                    .collect();
                inner.listings.retain(|path, _| visited.contains(path));
            }
            inner.status = progress.clone();
        }
        for path in removed {
            let _ = app.emit_to(
                EventTarget::webview("main"),
                "directory-cache-updated",
                serde_json::json!({"path":path}),
            );
        }
        let _ = app.emit_to(EventTarget::webview("main"), "directory-scan", progress);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn listing(path: &str) -> Listing {
        Listing {
            path: path.into(),
            entries: Vec::new(),
            cached: false,
            updated_at: now(),
        }
    }
    fn cache() -> DirectoryCache {
        let cache = DirectoryCache::default();
        {
            let mut inner = cache.inner.lock().unwrap();
            inner.status = ScanStatus {
                enabled: true,
                root: "/home/user".into(),
                depth: 3,
                ..Default::default()
            };
        }
        cache
    }
    #[test]
    fn depth_is_relative_and_component_bounded() {
        assert!(contains("/home/user", "/home/user", 0));
        assert!(!contains("/home/user", "/home/user/a", 0));
        assert!(contains("/home/user", "/home/user/a/b", 2));
        assert!(!contains("/home/user", "/home/user/a/b/c", 2));
        assert!(!contains("/home/user", "/home/username/a", 3));
        assert!(contains("/", "/a/b", 2));
    }
    #[test]
    fn invalidation_removes_parents_and_moved_subtrees() {
        let cache = cache();
        for p in [
            "/home/user",
            "/home/user/a",
            "/home/user/a/deep",
            "/home/user/b",
        ] {
            cache.remember(listing(p));
        }
        cache.invalidate(&["/home/user/a"]);
        assert!(cache.get("/home/user").is_none());
        assert!(cache.get("/home/user/a/deep").is_none());
        assert!(cache.get("/home/user/b").unwrap().cached);
    }
    #[test]
    fn cache_is_scoped_and_stop_discards_snapshots() {
        let cache = cache();
        cache.remember(listing("/etc"));
        assert!(cache.get("/etc").is_none());
        cache.remember(listing("/home/user/a"));
        assert!(cache.get("/home/user/a/").is_some());
        cache.stop();
        assert!(!cache.status().enabled);
        assert!(cache.get("/home/user/a").is_none());
    }
    #[test]
    fn refresh_only_reports_content_changes() {
        let cache = cache();
        assert!(!cache.remember(listing("/etc")));
        assert!(cache.remember(listing("/home/user")));
        let mut refreshed = listing("/home/user");
        refreshed.updated_at += 10;
        assert!(!cache.remember(refreshed));
        cache.invalidate(&["/home/user/new-file"]);
        assert!(cache.remember(listing("/home/user")));
    }
}
