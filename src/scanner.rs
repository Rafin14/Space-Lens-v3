use serde::{Deserialize, Serialize};
use std::os::windows::fs::MetadataExt;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime},
};
use unicode_casefold::UnicodeCaseFold;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub size: Option<u64>,
    pub is_dir: bool,
    pub partial: bool,
    #[serde(default)]
    pub file_id: u64,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Listing {
    pub path: PathBuf,
    pub entries: Arc<Vec<Entry>>,
    pub total: u64,
    pub errors: usize,
    pub error: String,
    #[serde(default)]
    pub file_id: u64,
    #[serde(default)]
    pub direct_errors: usize,
    #[serde(default)]
    pub files_direct: usize,
    #[serde(default)]
    pub links_direct: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub root: PathBuf,
    pub listings: Vec<Listing>,
    pub index: HashMap<PathBuf, usize>,
    pub measured_at: SystemTime,
    pub files: usize,
    pub links_skipped: usize,
    pub issues: Vec<(PathBuf, String)>,
    #[serde(default)]
    pub scan_info: ScanInfo,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScanInfo {
    pub engine: String,
    pub workers: usize,
    pub directories_read: usize,
    pub incremental: bool,
    pub fallback: Option<String>,
}
impl Snapshot {
    pub fn empty(root: PathBuf) -> Self {
        Self {
            root,
            listings: vec![],
            index: HashMap::new(),
            measured_at: SystemTime::now(),
            files: 0,
            links_skipped: 0,
            issues: vec![],
            scan_info: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Default, Serialize)]
pub struct Progress {
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
}

#[derive(Debug)]
pub enum ScanError {
    Cancelled,
    Io(String),
}

pub fn normalize(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

pub fn within(path: &Path, root: &Path) -> bool {
    let a = normalize(path);
    let b = normalize(root);
    let mut parts = a.components();
    b.components().all(|part| {
        parts.next().is_some_and(|other| {
            other.as_os_str().to_string_lossy().to_lowercase()
                == part.as_os_str().to_string_lossy().to_lowercase()
        })
    })
}

pub fn is_link(info: &fs::Metadata) -> bool {
    info.file_attributes() & 0x400 != 0
}

fn check(cancel: &AtomicBool) -> Result<(), ScanError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ScanError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn scan(
    root: &Path,
    cancel: &AtomicBool,
    progress: Option<&(dyn Fn(Progress) + Sync)>,
) -> Result<Snapshot, ScanError> {
    crate::engine::Scanner::new(crate::engine::options()).scan(root, cancel, progress)
}

pub fn scan_reference(
    root: &Path,
    cancel: &AtomicBool,
    progress: Option<&dyn Fn(Progress)>,
) -> Result<Snapshot, ScanError> {
    check(cancel)?;
    let root = normalize(root);
    let info = fs::symlink_metadata(&root).map_err(|e| ScanError::Io(e.to_string()))?;
    if is_link(&info) {
        return Err(ScanError::Io(
            "Choose a real folder. Linked folders and Windows reparse points are excluded.".into(),
        ));
    }
    if !info.is_dir() {
        return Err(ScanError::Io("Choose a folder to inspect.".into()));
    }
    let mut result = Snapshot {
        root: root.clone(),
        listings: vec![],
        index: HashMap::new(),
        measured_at: SystemTime::now(),
        files: 0,
        links_skipped: 0,
        issues: vec![],
        scan_info: ScanInfo {
            engine: "serial".into(),
            workers: 1,
            ..Default::default()
        },
    };
    let mut stack = vec![root.clone()];
    let mut bytes_seen = 0;
    let mut last_progress = Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .unwrap();
    while let Some(path) = stack.pop() {
        check(cancel)?;
        let mut listing = Listing {
            path: path.clone(),
            ..Default::default()
        };
        let children = fs::symlink_metadata(&path).and_then(|info| {
            if is_link(&info) {
                return Err(std::io::Error::other(
                    "Folder changed into an excluded link during the scan",
                ));
            }
            fs::read_dir(&path)
        });
        match children {
            Ok(children) => {
                for child in children {
                    check(cancel)?;
                    match child {
                        Ok(child) => {
                            let child_path = child.path();
                            let name = child.file_name().to_string_lossy().into_owned();
                            let directory = child.file_type().is_ok_and(|kind| kind.is_dir());
                            match child.metadata() {
                                Ok(info) => {
                                    if is_link(&info) {
                                        result.links_skipped += 1;
                                        listing.links_direct += 1;
                                        continue;
                                    }
                                    if directory {
                                        stack.push(child_path.clone());
                                        Arc::make_mut(&mut listing.entries).push(Entry {
                                            path: child_path,
                                            name,
                                            size: Some(0),
                                            is_dir: true,
                                            partial: false,
                                            file_id: 0,
                                        });
                                    } else if info.is_file() {
                                        Arc::make_mut(&mut listing.entries).push(Entry {
                                            path: child_path,
                                            name,
                                            size: Some(info.len()),
                                            is_dir: false,
                                            partial: false,
                                            file_id: 0,
                                        });
                                        result.files += 1;
                                        listing.files_direct += 1;
                                        bytes_seen += info.len();
                                    }
                                }
                                Err(error) => {
                                    listing.errors += 1;
                                    if result.issues.len() < 100 {
                                        result.issues.push((child_path.clone(), error.to_string()));
                                    }
                                    Arc::make_mut(&mut listing.entries).push(Entry {
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
                        Err(error) => {
                            listing.errors += 1;
                            if result.issues.len() < 100 {
                                result.issues.push((path.clone(), error.to_string()));
                            }
                        }
                    }
                    if let Some(callback) = progress {
                        if last_progress.elapsed().as_millis() >= 150 {
                            callback(Progress {
                                files: result.files,
                                folders: result.listings.len() + 1,
                                bytes: bytes_seen,
                            });
                            last_progress = Instant::now();
                        }
                    }
                }
            }
            Err(error) => {
                if path == root {
                    return Err(ScanError::Io(error.to_string()));
                }
                listing.error = error.to_string();
                listing.errors += 1;
                if result.issues.len() < 100 {
                    result.issues.push((path.clone(), error.to_string()));
                }
            }
        }
        listing.direct_errors = listing.errors;
        result.index.insert(path, result.listings.len());
        result.listings.push(listing);
    }
    for i in (0..result.listings.len()).rev() {
        check(cancel)?;
        let mut owned = std::mem::take(&mut result.listings[i].entries);
        let entries = Arc::make_mut(&mut owned);
        let mut child_errors = 0;
        for entry in entries.iter_mut() {
            if entry.is_dir {
                if let Some(&child) = result.index.get(&entry.path) {
                    let listing = &result.listings[child];
                    entry.size = if !listing.error.is_empty() && listing.entries.is_empty() {
                        None
                    } else {
                        Some(listing.total)
                    };
                    entry.partial = listing.errors != 0;
                    child_errors += listing.errors;
                }
            }
        }
        entries.sort_by_cached_key(|e| {
            (
                e.size.is_none(),
                std::cmp::Reverse(e.size.unwrap_or(0)),
                e.name.case_fold().collect::<String>(),
            )
        });
        let listing = &mut result.listings[i];
        listing.total = entries.iter().map(|e| e.size.unwrap_or(0)).sum();
        listing.errors += child_errors;
        listing.entries = owned;
    }
    result.scan_info.directories_read = result.listings.len();
    result.measured_at = SystemTime::now();
    if let Some(callback) = progress {
        callback(Progress {
            files: result.files,
            folders: result.listings.len(),
            bytes: bytes_seen,
        });
    }
    Ok(result)
}

pub fn size_text(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "Unknown".into();
    };
    let mut amount = bytes as f64;
    for unit in ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] {
        if amount < 1024.0 || unit == "PiB" {
            if unit == "B" {
                return format!("{} B", commas(bytes));
            }
            let number = format!("{amount:.1}");
            let (integer, fraction) = number.split_once('.').unwrap();
            return format!("{}.{} {unit}", commas(integer.parse().unwrap()), fraction);
        }
        amount /= 1024.0;
    }
    unreachable!()
}

pub fn commas(number: u64) -> String {
    let text = number.to_string();
    text.chars()
        .enumerate()
        .fold(String::new(), |mut out, (i, c)| {
            if i > 0 && (text.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
            out
        })
}
