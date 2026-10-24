use crate::scanner::{self, Entry, Listing, Progress, ScanError, ScanInfo, Snapshot};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};
use unicode_casefold::UnicodeCaseFold;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Serial,
    Parallel,
    Indexed,
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub engine: Engine,
    pub workers: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            engine: Engine::Parallel,
            workers: std::thread::available_parallelism()
                .map_or(2, |n| n.get())
                .min(4),
        }
    }
}
static OPTIONS: OnceLock<Options> = OnceLock::new();
pub fn configure(options: Options) {
    let _ = OPTIONS.set(Options {
        workers: options.workers.clamp(1, 8),
        ..options
    });
}
pub fn options() -> Options {
    *OPTIONS.get_or_init(Options::default)
}
pub fn parse_options(args: &[String]) -> Options {
    let mut result = Options::default();
    for pair in args.windows(2) {
        match pair[0].as_str() {
            "--engine" => {
                result.engine = match pair[1].as_str() {
                    "serial" => Engine::Serial,
                    "indexed" => Engine::Indexed,
                    _ => Engine::Parallel,
                }
            }
            "--workers" => result.workers = pair[1].parse::<usize>().unwrap_or(4).clamp(1, 8),
            _ => {}
        }
    }
    result
}
#[derive(Default)]
struct Counts {
    files: AtomicUsize,
    folders: AtomicUsize,
    bytes: AtomicU64,
}
impl Counts {
    fn get(&self) -> Progress {
        Progress {
            files: self.files.load(Ordering::Relaxed),
            folders: self.folders.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}
struct Directory {
    listing: Listing,
    children: Vec<PathBuf>,
    issues: Vec<(PathBuf, String)>,
}
fn read_directory(
    path: &Path,
    cancel: &AtomicBool,
    counts: &Counts,
    indexed: bool,
) -> Result<Directory, ScanError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled);
    }
    counts.folders.fetch_add(1, Ordering::Relaxed);
    let mut data = Directory {
        listing: Listing {
            path: path.to_path_buf(),
            ..Default::default()
        },
        children: vec![],
        issues: vec![],
    };
    let check = fs::symlink_metadata(path).and_then(|info| {
        if scanner::is_link(&info) {
            Err(std::io::Error::other(
                "Folder changed into an excluded link during the scan",
            ))
        } else {
            Ok(())
        }
    });
    if let Err(error) = check {
        data.listing.error = error.to_string();
        data.listing.direct_errors = 1;
        data.issues.push((path.to_path_buf(), error.to_string()));
        return Ok(data);
    }
    if indexed {
        if let Ok((id, entries)) = crate::ntfs::directory_entries(path, cancel) {
            data.listing.file_id = id;
            for entry in entries {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ScanError::Cancelled);
                }
                if entry.reparse {
                    data.listing.links_direct += 1;
                    continue;
                }
                let child = path.join(&entry.name);
                if entry.directory {
                    data.children.push(child.clone());
                } else {
                    data.listing.files_direct += 1;
                    counts.files.fetch_add(1, Ordering::Relaxed);
                    counts.bytes.fetch_add(entry.size, Ordering::Relaxed);
                }
                Arc::make_mut(&mut data.listing.entries).push(Entry {
                    path: child,
                    name: entry.name,
                    size: Some(if entry.directory { 0 } else { entry.size }),
                    is_dir: entry.directory,
                    partial: false,
                    file_id: entry.id,
                });
            }
            return Ok(data);
        }
    }
    match fs::read_dir(path) {
        Err(error) => {
            data.listing.error = error.to_string();
            data.listing.direct_errors = 1;
            data.issues.push((path.to_path_buf(), error.to_string()));
        }
        Ok(entries) => {
            for child in entries {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ScanError::Cancelled);
                }
                match child {
                    Err(error) => {
                        data.listing.direct_errors += 1;
                        if data.issues.len() < 100 {
                            data.issues.push((path.to_path_buf(), error.to_string()));
                        }
                    }
                    Ok(child) => {
                        let child_path = child.path();
                        let name = child.file_name().to_string_lossy().into_owned();
                        let directory = child.file_type().is_ok_and(|t| t.is_dir());
                        match child.metadata() {
                            Ok(info) => {
                                if scanner::is_link(&info) {
                                    data.listing.links_direct += 1;
                                    continue;
                                }
                                if directory {
                                    data.children.push(child_path.clone());
                                } else if !info.is_file() {
                                    continue;
                                } else {
                                    data.listing.files_direct += 1;
                                    counts.files.fetch_add(1, Ordering::Relaxed);
                                    counts.bytes.fetch_add(info.len(), Ordering::Relaxed);
                                }
                                Arc::make_mut(&mut data.listing.entries).push(Entry {
                                    path: child_path,
                                    name,
                                    size: Some(if directory { 0 } else { info.len() }),
                                    is_dir: directory,
                                    partial: false,
                                    file_id: 0,
                                });
                            }
                            Err(error) => {
                                data.listing.direct_errors += 1;
                                if data.issues.len() < 100 {
                                    data.issues.push((child_path.clone(), error.to_string()));
                                }
                                Arc::make_mut(&mut data.listing.entries).push(Entry {
                                    path: child_path,
                                    name,
                                    size: None,
                                    is_dir: directory,
                                    partial: true,
                                    file_id: 0,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(data)
}

pub fn finalize(
    snapshot: &mut Snapshot,
    dirty: Option<&HashSet<PathBuf>>,
    cancel: &AtomicBool,
) -> Result<(), ScanError> {
    snapshot
        .listings
        .sort_by_key(|l| l.path.components().count());
    snapshot.index = snapshot
        .listings
        .iter()
        .enumerate()
        .map(|(i, l)| (l.path.clone(), i))
        .collect();
    for i in (0..snapshot.listings.len()).rev() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        if dirty.is_some_and(|d| !d.contains(&snapshot.listings[i].path)) {
            continue;
        }
        let mut owned = std::mem::take(&mut snapshot.listings[i].entries);
        let entries = Arc::make_mut(&mut owned);
        let mut errors = snapshot.listings[i].direct_errors;
        for entry in entries.iter_mut().filter(|e| e.is_dir) {
            if let Some(&child) = snapshot.index.get(&entry.path) {
                let listing = &snapshot.listings[child];
                entry.size = if !listing.error.is_empty() && listing.entries.is_empty() {
                    None
                } else {
                    Some(listing.total)
                };
                entry.partial = listing.errors > 0;
                errors += listing.errors;
            }
        }
        entries.sort_by_cached_key(|e| {
            (
                e.size.is_none(),
                std::cmp::Reverse(e.size.unwrap_or(0)),
                if e.name.is_ascii() {
                    e.name.to_ascii_lowercase()
                } else {
                    e.name.case_fold().collect::<String>()
                },
            )
        });
        snapshot.listings[i].total = entries.iter().map(|e| e.size.unwrap_or(0)).sum();
        snapshot.listings[i].errors = errors;
        snapshot.listings[i].entries = owned;
    }
    snapshot.files = snapshot.listings.iter().map(|l| l.files_direct).sum();
    snapshot.links_skipped = snapshot.listings.iter().map(|l| l.links_direct).sum();
    snapshot.measured_at = SystemTime::now();
    Ok(())
}
fn parallel(
    root: &Path,
    cancel: &AtomicBool,
    progress: Option<&(dyn Fn(Progress) + Sync)>,
    options: Options,
) -> Result<Snapshot, ScanError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled);
    }
    let root = scanner::normalize(root);
    let info = fs::symlink_metadata(&root).map_err(|e| ScanError::Io(e.to_string()))?;
    if scanner::is_link(&info) || !info.is_dir() {
        return Err(ScanError::Io(
            "Choose a real folder. Links and reparse points are excluded.".into(),
        ));
    }
    let workers = options.workers.clamp(1, 8);
    let queue = Arc::new((Mutex::new((vec![root.clone()], false)), Condvar::new()));
    let counts = Arc::new(Counts::default());
    let (tx, rx) = mpsc::channel();
    let mut snapshot = Snapshot::empty(root.clone());
    let result = std::thread::scope(|scope| -> Result<(), ScanError> {
        for _ in 0..workers {
            let queue = queue.clone();
            let counts = counts.clone();
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let path = {
                        let (lock, ready) = &*queue;
                        let mut state = lock.lock().unwrap();
                        while state.0.is_empty() && !state.1 {
                            state = ready.wait(state).unwrap();
                        }
                        if state.1 {
                            return;
                        }
                        state.0.pop().unwrap()
                    };
                    let result =
                        read_directory(&path, cancel, &counts, options.engine == Engine::Indexed);
                    if tx.send(result).is_err() {
                        return;
                    }
                }
            });
        }
        let mut remaining = 1;
        let mut last = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        let mut failure = None;
        while remaining > 0 {
            if cancel.load(Ordering::Relaxed) {
                failure = Some(ScanError::Cancelled);
                break;
            }
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(Ok(data)) => {
                    remaining -= 1;
                    if data.listing.path == root && !data.listing.error.is_empty() {
                        failure = Some(ScanError::Io(data.listing.error));
                        break;
                    }
                    remaining += data.children.len();
                    snapshot.listings.push(data.listing);
                    let available = 100usize.saturating_sub(snapshot.issues.len());
                    snapshot
                        .issues
                        .extend(data.issues.into_iter().take(available));
                    if !data.children.is_empty() {
                        let (lock, ready) = &*queue;
                        lock.lock().unwrap().0.extend(data.children);
                        ready.notify_all();
                    }
                }
                Ok(Err(error)) => {
                    failure = Some(error);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => {
                    failure = Some(ScanError::Io("Scanner workers disconnected".into()));
                    break;
                }
            }
            if last.elapsed() >= Duration::from_millis(150) {
                if let Some(callback) = progress {
                    callback(counts.get());
                }
                last = Instant::now();
            }
        }
        let (lock, ready) = &*queue;
        lock.lock().unwrap().1 = true;
        ready.notify_all();
        if let Some(error) = failure {
            Err(error)
        } else {
            Ok(())
        }
    });
    result?;
    finalize(&mut snapshot, None, cancel)?;
    snapshot.scan_info = ScanInfo {
        engine: if options.engine == Engine::Indexed {
            "indexed"
        } else {
            "parallel"
        }
        .into(),
        workers,
        directories_read: snapshot.listings.len(),
        ..Default::default()
    };
    if let Some(callback) = progress {
        callback(Progress {
            files: snapshot.files,
            folders: snapshot.listings.len(),
            bytes: snapshot.listings[0].total,
        });
    }
    Ok(snapshot)
}

pub struct Scanner {
    options: Options,
    index: Option<crate::ntfs::JournalIndex>,
}
impl Scanner {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            index: None,
        }
    }
    pub fn scan(
        &mut self,
        root: &Path,
        cancel: &AtomicBool,
        progress: Option<&(dyn Fn(Progress) + Sync)>,
    ) -> Result<Snapshot, ScanError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        if self.options.engine == Engine::Serial {
            return scanner::scan_reference(root, cancel, progress.map(|p| p as &dyn Fn(Progress)));
        }
        let metadata = fs::symlink_metadata(root).map_err(|e| ScanError::Io(e.to_string()))?;
        if !metadata.is_dir() || scanner::is_link(&metadata) {
            return Err(ScanError::Io(
                "Choose a real folder. Links and reparse points are excluded.".into(),
            ));
        }
        let mark = if self.options.engine == Engine::Indexed {
            crate::ntfs::journal_mark(root).ok()
        } else {
            None
        };
        let mut fallback = None;
        if let (Some(index), Some(mark)) = (&self.index, &mark) {
            match index.dirty(root, mark, cancel) {
                Ok(dirty) => {
                    let mut snapshot =
                        refresh_directories(&index.snapshot, &dirty, &[], cancel, self.options)?;
                    snapshot.scan_info.engine = "indexed".into();
                    snapshot.scan_info.incremental = true;
                    self.index =
                        crate::ntfs::JournalIndex::new(Arc::new(snapshot.clone()), mark.clone());
                    return Ok(snapshot);
                }
                Err(error) => fallback = Some(error),
            }
        } else if self.options.engine == Engine::Indexed {
            fallback = Some("No usable NTFS journal cache; normal enumeration was used".into());
        }
        let mut snapshot = parallel(root, cancel, progress, self.options)?;
        snapshot.scan_info.fallback = fallback;
        if let Some(mark) = mark {
            self.index = crate::ntfs::JournalIndex::new(Arc::new(snapshot.clone()), mark);
        } else {
            self.index = None;
        }
        Ok(snapshot)
    }
    pub fn refresh_after_delete(
        &mut self,
        previous: &Arc<Snapshot>,
        target: &Path,
        cancel: &AtomicBool,
    ) -> Result<Snapshot, ScanError> {
        self.index = None;
        let parent = target
            .parent()
            .filter(|p| scanner::within(p, &previous.root))
            .unwrap_or(&previous.root)
            .to_path_buf();
        let full = if target.is_dir() {
            vec![target.to_path_buf()]
        } else {
            vec![]
        };
        refresh_directories(previous, &[parent], &full, cancel, self.options)
    }
}

fn ancestor_paths(path: &Path, root: &Path, dirty: &mut HashSet<PathBuf>) {
    let mut cursor = Some(path);
    while let Some(p) = cursor {
        if !scanner::within(p, root) {
            break;
        }
        dirty.insert(p.to_path_buf());
        if scanner::same_path(p, root) {
            break;
        }
        cursor = p.parent();
    }
}
pub(crate) fn refresh_directories(
    previous: &Arc<Snapshot>,
    paths: &[PathBuf],
    full: &[PathBuf],
    cancel: &AtomicBool,
    options: Options,
) -> Result<Snapshot, ScanError> {
    if previous.listings.first().is_none_or(|l| l.errors > 0) {
        return parallel(&previous.root, cancel, None, options);
    }
    let mut snapshot = (**previous).clone();
    if paths.is_empty() && full.is_empty() {
        snapshot.measured_at = SystemTime::now();
        snapshot.scan_info = ScanInfo {
            engine: "targeted".into(),
            workers: options.workers,
            directories_read: 0,
            incremental: true,
            fallback: None,
        };
        return Ok(snapshot);
    }
    let mut dirty = HashSet::new();
    let mut removed = vec![];
    let mut read = 0;
    for path in full {
        if scanner::within(path, &previous.root) && path.is_dir() {
            let replacement = parallel(path, cancel, None, options)?;
            read += replacement.listings.len();
            removed.push(path.clone());
            snapshot.issues.extend(replacement.issues);
            for listing in replacement.listings {
                dirty.insert(listing.path.clone());
                snapshot.listings.push(listing);
            }
        }
    }
    if !removed.is_empty() {
        let original = previous.listings.len();
        let mut position = 0;
        snapshot.listings.retain(|l| {
            let keep = position >= original || !removed.iter().any(|p| scanner::within(&l.path, p));
            position += 1;
            keep
        });
    }
    let mut existing: HashSet<PathBuf> = snapshot.listings.iter().map(|l| l.path.clone()).collect();
    for path in paths {
        if !scanner::within(path, &previous.root) {
            return Err(ScanError::Io("Refresh escaped the scanned root".into()));
        }
        let counts = Counts::default();
        let data = read_directory(path, cancel, &counts, options.engine == Engine::Indexed)?;
        read += 1;
        if path == &previous.root && !data.listing.error.is_empty() {
            return Err(ScanError::Io(data.listing.error));
        }
        let old_children = previous
            .index
            .get(path)
            .map(|&i| {
                previous.listings[i]
                    .entries
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for old in old_children {
            if !data.children.contains(&old) {
                snapshot
                    .listings
                    .retain(|l| !scanner::within(&l.path, &old));
                existing.retain(|p| !scanner::within(p, &old));
            }
        }
        snapshot.listings.retain(|l| l.path != *path);
        snapshot.issues.extend(
            data.issues
                .into_iter()
                .take(100usize.saturating_sub(snapshot.issues.len())),
        );
        for child in data.children {
            if !existing.contains(&child) {
                let replacement = parallel(&child, cancel, None, options)?;
                read += replacement.listings.len();
                for listing in replacement.listings {
                    dirty.insert(listing.path.clone());
                    existing.insert(listing.path.clone());
                    snapshot.listings.push(listing);
                }
                snapshot.issues.extend(
                    replacement
                        .issues
                        .into_iter()
                        .take(100usize.saturating_sub(snapshot.issues.len())),
                );
            }
        }
        snapshot.listings.push(data.listing);
        ancestor_paths(path, &previous.root, &mut dirty);
    }
    for path in full {
        ancestor_paths(path, &previous.root, &mut dirty);
    }
    finalize(&mut snapshot, Some(&dirty), cancel)?;
    snapshot.scan_info = ScanInfo {
        engine: "targeted".into(),
        workers: options.workers,
        directories_read: read,
        incremental: true,
        fallback: None,
    };
    Ok(snapshot)
}
