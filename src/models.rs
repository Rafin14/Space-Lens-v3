use crate::{
    scanner::{Entry, Snapshot, commas, size_text},
    view::{EntryView, Row},
};
use std::{
    cell::RefCell,
    ffi::{CString, c_char},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Folders,
    Files,
    All,
}
impl Mode {
    pub fn allows(self, entry: &Entry) -> bool {
        self == Self::All || entry.is_dir == (self == Self::Folders)
    }
}
#[derive(Default)]
pub struct ModelStore {
    pub snapshot: Option<Arc<Snapshot>>,
    pub directory: usize,
    pub main: Vec<usize>,
    pub expanded: Vec<usize>,
    pub children: Vec<Vec<usize>>,
    pub parents: Vec<(usize, usize)>,
}
thread_local! {pub static STORE:RefCell<ModelStore>=RefCell::new(ModelStore::default());}

pub fn publish(snapshot: Arc<Snapshot>, current: &Path, main: Mode, expanded: Mode) -> bool {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        let changed = !store
            .snapshot
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &snapshot));
        if changed {
            store.children = vec![vec![]; snapshot.listings.len()];
            store.parents = vec![(0, 0); snapshot.listings.len()];
            for (i, listing) in snapshot.listings.iter().enumerate() {
                for entry in listing.entries.iter().filter(|e| e.is_dir) {
                    if let Some(&child) = snapshot.index.get(&entry.path) {
                        let row = store.children[i].len();
                        store.parents[child] = (i + 1, row);
                        store.children[i].push(child + 1);
                    }
                }
            }
        }
        store.directory = *snapshot.index.get(current).unwrap_or(&0);
        store.main = snapshot.listings[store.directory]
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| main.allows(e))
            .map(|(i, _)| i)
            .collect();
        store.expanded = snapshot.listings[store.directory]
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| expanded.allows(e))
            .map(|(i, _)| i)
            .collect();
        store.snapshot = Some(snapshot);
        changed
    })
}
pub fn clear() {
    STORE.with(|store| *store.borrow_mut() = ModelStore::default());
}
pub fn entry(model: i32, row: i32) -> Option<Entry> {
    STORE.with(|store| {
        let store = store.borrow();
        let snapshot = store.snapshot.as_ref()?;
        let rows = if model == 2 {
            &store.expanded
        } else {
            &store.main
        };
        let index = *rows.get(usize::try_from(row).ok()?)?;
        Some(snapshot.listings[store.directory].entries[index].clone())
    })
}
pub fn entries(model: i32) -> Vec<Entry> {
    STORE.with(|store| {
        let store = store.borrow();
        let Some(snapshot) = store.snapshot.as_ref() else {
            return vec![];
        };
        let rows = if model == 2 {
            &store.expanded
        } else {
            &store.main
        };
        rows.iter()
            .map(|&i| snapshot.listings[store.directory].entries[i].clone())
            .collect()
    })
}
pub fn view(model: i32) -> EntryView {
    STORE.with(|store| {
        let store = store.borrow();
        let rows = if model == 2 {
            &store.expanded
        } else {
            &store.main
        };
        EntryView {
            snapshot: store.snapshot.clone(),
            directory: store.directory,
            rows: Arc::new(
                rows.iter()
                    .enumerate()
                    .map(|(table, &entry)| Row { entry, table })
                    .collect(),
            ),
            other: None,
        }
    })
}
pub fn row_count(model: i32) -> usize {
    STORE.with(|store| {
        let store = store.borrow();
        if model == 2 {
            store.expanded.len()
        } else {
            store.main.len()
        }
    })
}
pub fn tree_path(node: u64) -> Option<PathBuf> {
    STORE.with(|store| {
        store
            .borrow()
            .snapshot
            .as_ref()?
            .listings
            .get(node.checked_sub(1)? as usize)
            .map(|l| l.path.clone())
    })
}
pub fn tree_node(path: &Path) -> Option<u64> {
    STORE.with(|store| {
        store
            .borrow()
            .snapshot
            .as_ref()?
            .index
            .get(path)
            .map(|i| *i as u64 + 1)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_model_count(model: i32, parent: u64) -> i32 {
    STORE.with(|store| {
        let store = store.borrow();
        if store.snapshot.is_none() {
            return 0;
        }
        if model == 0 {
            if parent == 0 {
                1
            } else {
                store
                    .children
                    .get(parent as usize - 1)
                    .map_or(0, |v| v.len() as i32)
            }
        } else if parent != 0 {
            0
        } else if model == 2 {
            store.expanded.len() as i32
        } else {
            store.main.len() as i32
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_model_child(parent: u64, row: i32) -> u64 {
    STORE.with(|store| {
        let store = store.borrow();
        if parent == 0 {
            return if row == 0 && store.snapshot.is_some() {
                1
            } else {
                0
            };
        }
        store
            .children
            .get(parent as usize - 1)
            .and_then(|v| v.get(row as usize))
            .copied()
            .unwrap_or(0) as u64
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_model_parent(node: u64, row: *mut i32) -> u64 {
    STORE.with(|store| {
        let store = store.borrow();
        let (parent, index) = store
            .parents
            .get(node.saturating_sub(1) as usize)
            .copied()
            .unwrap_or((0, 0));
        unsafe {
            if !row.is_null() {
                *row = index as i32;
            }
        }
        parent as u64
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_model_data(
    model: i32,
    node: u64,
    row: i32,
    column: i32,
    role: i32,
) -> *mut c_char {
    let text = STORE.with(|store| {
        let store = store.borrow();
        let Some(snapshot) = store.snapshot.as_ref() else {
            return String::new();
        };
        if model == 0 {
            let Some(listing) = snapshot.listings.get(node.saturating_sub(1) as usize) else {
                return String::new();
            };
            if role == 3 || role == 12 {
                return listing.path.to_string_lossy().into_owned();
            }
            if column == 0 {
                return listing.path.file_name().map_or_else(
                    || listing.path.to_string_lossy().into_owned(),
                    |n| n.to_string_lossy().into_owned(),
                );
            }
            let size = if !listing.error.is_empty() && listing.entries.is_empty() {
                None
            } else {
                Some(listing.total)
            };
            return size_text(size)
                + if listing.errors > 0 && size.is_some() {
                    " +"
                } else {
                    ""
                };
        }
        let rows = if model == 2 {
            &store.expanded
        } else {
            &store.main
        };
        let Some(&index) = rows.get(row as usize) else {
            return String::new();
        };
        let listing = &snapshot.listings[store.directory];
        let entry = &listing.entries[index];
        if role == 100 {
            return if entry.is_dir { "1" } else { "0" }.into();
        }
        if role == 101 {
            return if entry.partial { "1" } else { "0" }.into();
        }
        if role == 3 || role == 12 {
            return format!(
                "{}\n{}",
                entry.path.display(),
                entry
                    .size
                    .map_or("Unknown size".into(), |n| format!("{} bytes", commas(n)))
            );
        }
        match column {
            0 => entry.name.clone(),
            1 => {
                size_text(entry.size)
                    + if entry.partial && entry.size.is_some() {
                        " +"
                    } else {
                        ""
                    }
            }
            2 => {
                if listing.errors > 0 || entry.size.is_none() {
                    "—".into()
                } else if listing.total == 0 {
                    "0.0%".into()
                } else {
                    format!(
                        "{:0.1}%",
                        entry.size.unwrap() as f64 / listing.total as f64 * 100.
                    )
                }
            }
            3 => if entry.size.is_none() {
                "Unreadable"
            } else if entry.partial {
                "Partial"
            } else {
                "Measured"
            }
            .into(),
            _ => String::new(),
        }
    });
    CString::new(text.replace('\0', "�")).unwrap().into_raw()
}
#[unsafe(no_mangle)]
pub extern "C" fn rust_string_free(text: *mut c_char) {
    if !text.is_null() {
        unsafe {
            drop(CString::from_raw(text));
        }
    }
}
