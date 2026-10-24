//! Optional, read-only NTFS file-ID enumeration and journal-validated refreshes.
//! This is not a raw-MFT parser. Initial scans enumerate directories normally; refreshes reuse verified metadata.
use crate::scanner::{self, Snapshot};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE},
        Storage::FileSystem::*,
        System::{IO::DeviceIoControl, Ioctl::*},
    },
    core::PCWSTR,
};
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn u16_at(data: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .ok_or("Truncated native record")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(data: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .ok_or("Truncated native record")?
            .try_into()
            .unwrap(),
    ))
}
fn u64_at(data: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        data.get(at..at + 8)
            .ok_or("Truncated native record")?
            .try_into()
            .unwrap(),
    ))
}
pub struct NativeEntry {
    pub name: String,
    pub id: u64,
    pub size: u64,
    pub directory: bool,
    pub reparse: bool,
}
pub fn directory_entries(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(u64, Vec<NativeEntry>), String> {
    unsafe {
        let h = Handle(
            CreateFileW(
                PCWSTR(wide(path).as_ptr()),
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
            .map_err(|e| e.to_string())?,
        );
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        GetFileInformationByHandle(h.0, &mut info).map_err(|e| e.to_string())?;
        if info.dwFileAttributes & 0x400 != 0 {
            return Err("Directory became a reparse point".into());
        }
        let id = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
        let mut result = vec![];
        let mut buffer = vec![0u64; 8192];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            if let Err(error) = GetFileInformationByHandleEx(
                h.0,
                FileIdBothDirectoryInfo,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
            ) {
                if error.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) {
                    break;
                }
                return Err(error.to_string());
            }
            let data = std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), buffer.len() * 8);
            let mut at = 0;
            loop {
                let next = u32_at(data, at)? as usize;
                let attr = u32_at(data, at + 56)?;
                let length = u32_at(data, at + 60)? as usize;
                if length % 2 != 0 || at + 104 + length > data.len() {
                    return Err("Invalid native directory record".into());
                }
                let name = String::from_utf16_lossy(
                    &data[at + 104..at + 104 + length]
                        .chunks_exact(2)
                        .map(|v| u16::from_le_bytes([v[0], v[1]]))
                        .collect::<Vec<_>>(),
                );
                if name != "." && name != ".." {
                    result.push(NativeEntry {
                        name,
                        id: u64_at(data, at + 96)?,
                        size: u64_at(data, at + 40)?,
                        directory: attr & 0x10 != 0,
                        reparse: attr & 0x400 != 0,
                    });
                }
                if next == 0 {
                    break;
                }
                if next < 104 || at + next >= data.len() {
                    return Err("Invalid native directory offset".into());
                }
                at += next;
            }
        }
        Ok((id, result))
    }
}
#[derive(Clone, Debug)]
pub struct JournalMark {
    volume: String,
    pub id: u64,
    pub first: i64,
    pub next: i64,
    pub lowest: i64,
}
fn volume_name(root: &Path) -> Result<(String, Vec<u16>), String> {
    let path = scanner::normalize(root).to_string_lossy().into_owned();
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err("NTFS indexing requires a local drive path".into());
    }
    let drive = format!("{}:\\", bytes[0] as char);
    let mut fs = [0u16; 32];
    unsafe {
        GetVolumeInformationW(
            PCWSTR(wide(Path::new(&drive)).as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut fs),
        )
        .map_err(|e| e.to_string())?;
    }
    if String::from_utf16_lossy(&fs[..fs.iter().position(|c| *c == 0).unwrap_or(fs.len())])
        != "NTFS"
    {
        return Err("Volume is not NTFS".into());
    }
    Ok((
        format!("\\\\.\\{}:", bytes[0] as char),
        wide(Path::new(&drive)),
    ))
}
fn volume_handle(volume: &str) -> Result<Handle, String> {
    unsafe {
        let name = wide(Path::new(volume));
        let open = |access| {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        open(0x80000000)
            .or_else(|_| open(0))
            .map(Handle)
            .map_err(|e| e.to_string())
    }
}
pub fn journal_mark(root: &Path) -> Result<JournalMark, String> {
    let (volume, _) = volume_name(root)?;
    let handle = volume_handle(&volume)?;
    let mut data = USN_JOURNAL_DATA_V0::default();
    let mut bytes = 0;
    unsafe {
        DeviceIoControl(
            handle.0,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some((&mut data as *mut USN_JOURNAL_DATA_V0).cast()),
            std::mem::size_of_val(&data) as u32,
            Some(&mut bytes),
            None,
        )
        .map_err(|e| e.to_string())?;
    }
    if bytes < std::mem::size_of_val(&data) as u32 {
        return Err("Incomplete journal information".into());
    }
    Ok(JournalMark {
        volume,
        id: data.UsnJournalID,
        first: data.FirstUsn,
        next: data.NextUsn,
        lowest: data.LowestValidUsn,
    })
}
#[derive(Clone, Copy)]
struct Change {
    id: u64,
    parent: u64,
    reason: u32,
    attrs: u32,
}
fn changes(
    from: &JournalMark,
    to: &JournalMark,
    cancel: &AtomicBool,
) -> Result<Vec<Change>, String> {
    if from.volume != to.volume
        || from.id != to.id
        || from.next < to.first.max(to.lowest)
        || from.next > to.next
    {
        return Err("Journal reset or retention gap requires a full scan".into());
    }
    let handle = volume_handle(&to.volume)?;
    let mut cursor = from.next;
    let mut result = vec![];
    let mut buffer = vec![0u64; 8192];
    let mut batches = 0;
    while cursor < to.next {
        batches += 1;
        if batches > 256 {
            return Err("Journal delta is too large; full scan used".into());
        }
        let request = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: cursor,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: to.id,
        };
        let mut used = 0;
        let read = |code, used: &mut u32, buffer: &mut Vec<u64>| unsafe {
            DeviceIoControl(
                handle.0,
                code,
                Some((&request as *const READ_USN_JOURNAL_DATA_V0).cast()),
                std::mem::size_of_val(&request) as u32,
                Some(buffer.as_mut_ptr().cast()),
                (buffer.len() * 8) as u32,
                Some(used),
                None,
            )
        };
        read(FSCTL_READ_USN_JOURNAL, &mut used, &mut buffer)
            .or_else(|_| read(FSCTL_READ_UNPRIVILEGED_USN_JOURNAL, &mut used, &mut buffer))
            .map_err(|e| e.to_string())?;
        if used < 8 {
            return Err("Incomplete journal read".into());
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), used as usize) };
        let next = u64_at(bytes, 0)? as i64;
        let mut offset = 8;
        while offset < bytes.len() {
            let length = u32_at(bytes, offset)? as usize;
            if length < 60 || offset + length > bytes.len() {
                return Err("Invalid journal record".into());
            }
            if u16_at(bytes, offset + 4)? != 2 {
                return Err("Unsupported journal record version; full scan used".into());
            }
            result.push(Change {
                id: u64_at(bytes, offset + 8)?,
                parent: u64_at(bytes, offset + 16)?,
                reason: u32_at(bytes, offset + 40)?,
                attrs: u32_at(bytes, offset + 52)?,
            });
            offset += length;
        }
        if next <= cursor {
            return Err("Journal read did not advance".into());
        }
        cursor = next;
    }
    Ok(result)
}
pub struct JournalIndex {
    pub snapshot: Arc<Snapshot>,
    mark: JournalMark,
    directories: HashMap<u64, usize>,
    parents: HashMap<u64, Vec<usize>>,
}
impl JournalIndex {
    pub fn new(snapshot: Arc<Snapshot>, mark: JournalMark) -> Option<Self> {
        if snapshot.listings.first()?.errors > 0
            || snapshot
                .listings
                .iter()
                .any(|l| l.file_id == 0 || l.entries.iter().any(|e| e.file_id == 0))
        {
            return None;
        }
        let mut directories = HashMap::new();
        let mut parents: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, listing) in snapshot.listings.iter().enumerate() {
            directories.insert(listing.file_id, i);
            for entry in listing.entries.iter() {
                parents.entry(entry.file_id).or_default().push(i);
            }
        }
        Some(Self {
            snapshot,
            mark,
            directories,
            parents,
        })
    }
    pub fn dirty(
        &self,
        root: &Path,
        to: &JournalMark,
        cancel: &AtomicBool,
    ) -> Result<Vec<PathBuf>, String> {
        if !scanner::same_path(root, &self.snapshot.root) {
            return Err("Scan root changed".into());
        }
        let records = changes(&self.mark, to, cancel)?;
        let mut dirty = HashSet::new();
        for record in records {
            if !self.directories.contains_key(&record.parent)
                && !self.directories.contains_key(&record.id)
                && !self.parents.contains_key(&record.id)
            {
                continue;
            }
            // Namespace/permission/link changes can affect paths outside an individual parent directory.
            if record.reason & (0x1000 | 0x2000 | 0x10000 | 0x100000 | 0x800) != 0
                || (record.attrs & 0x10 != 0 && record.reason & (0x100 | 0x200) != 0)
            {
                return Err(
                    "Directory, rename, link or permission changes require a full scan".into(),
                );
            }
            if let Some(&dir) = self.directories.get(&record.parent) {
                dirty.insert(self.snapshot.listings[dir].path.clone());
            }
            if let Some(parents) = self.parents.get(&record.id) {
                for &dir in parents {
                    dirty.insert(self.snapshot.listings[dir].path.clone());
                }
            }
        }
        Ok(dirty.into_iter().collect())
    }
}
