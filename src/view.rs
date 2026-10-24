use crate::scanner::{Entry, Snapshot};
use std::{ops::Index, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub entry: usize,
    pub table: usize,
}

/// A snapshot and small row indices are shared; names and paths are never copied for painting.
#[derive(Clone, Default)]
pub struct EntryView {
    pub snapshot: Option<Arc<Snapshot>>,
    pub directory: usize,
    pub rows: Arc<Vec<Row>>,
    pub other: Option<Entry>,
}
impl EntryView {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Entry> {
        (0..self.len()).map(|i| &self[i])
    }
    pub fn table_row(&self, index: usize) -> Option<i32> {
        self.rows
            .get(index)
            .filter(|r| r.entry != usize::MAX)
            .map(|r| r.table as i32)
    }
    pub fn subset(&self, positive: bool, limit: usize, aggregate: bool) -> Self {
        let mut rows = Vec::with_capacity(self.len().min(limit) + usize::from(aggregate));
        let mut remainder = 0;
        let mut count = 0;
        for row in self.rows.iter() {
            let entry =
                &self.snapshot.as_ref().unwrap().listings[self.directory].entries[row.entry];
            if positive && !entry.size.is_some_and(|s| s > 0) {
                continue;
            }
            if rows.len() < limit {
                rows.push(*row);
            } else if aggregate {
                remainder += entry.size.unwrap_or(0);
                count += 1;
            } else if limit != usize::MAX {
                break;
            }
        }
        let other = if count > 0 {
            rows.push(Row {
                entry: usize::MAX,
                table: usize::MAX,
            });
            Some(Entry {
                path: Default::default(),
                name: format!("Other {count} items"),
                size: Some(remainder),
                is_dir: false,
                partial: false,
                file_id: 0,
            })
        } else {
            None
        };
        Self {
            snapshot: self.snapshot.clone(),
            directory: self.directory,
            rows: Arc::new(rows),
            other,
        }
    }
}
impl Index<usize> for EntryView {
    type Output = Entry;
    fn index(&self, index: usize) -> &Entry {
        let row = self.rows[index];
        if row.entry == usize::MAX {
            self.other.as_ref().unwrap()
        } else {
            &self.snapshot.as_ref().unwrap().listings[self.directory].entries[row.entry]
        }
    }
}
