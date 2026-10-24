use crate::scanner::{is_link, normalize, same_path, within};
use std::{
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND},
        Storage::FileSystem::*,
        System::Com::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{Error, HRESULT, PCWSTR, Ref, Result as WinResult, implement},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub volume: u32,
    pub index: u64,
    pub created: u64,
}
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub fn target_identity(path: &Path, root: &Path) -> Result<Identity, String> {
    let path = normalize(path);
    let root = normalize(root);
    if same_path(&path, &root) || !within(&path, &root) {
        return Err(
            "Select an item inside the scanned folder. The scan root cannot be deleted.".into(),
        );
    }
    let mut current = path.clone();
    loop {
        let info = fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
        if is_link(&info) {
            return Err(
                "This path contains a link or junction. Refresh and choose a regular item.".into(),
            );
        }
        if same_path(&current, &root) {
            break;
        }
        current = current
            .parent()
            .ok_or("Path is outside the scanned folder")?
            .to_path_buf();
    }
    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide(&path).as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        let result = GetFileInformationByHandle(handle, &mut info);
        let _ = CloseHandle(handle);
        result.map_err(|e| e.to_string())?;
        Ok(Identity {
            volume: info.dwVolumeSerialNumber,
            index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
            created: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
                | info.ftCreationTime.dwLowDateTime as u64,
        })
    }
}

#[derive(Debug)]
pub enum DeleteError {
    Cancelled,
    Failure(String),
}
#[derive(Default)]
struct OperationResult {
    error: String,
    result: HRESULT,
}
#[implement(IFileOperationProgressSink)]
struct Sink {
    path: PathBuf,
    root: PathBuf,
    expected_identity: Identity,
    result: Arc<Mutex<OperationResult>>,
}
#[allow(non_snake_case, unused_variables)]
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn StartOperations(&self) -> WinResult<()> {
        Ok(())
    }
    fn FinishOperations(&self, result: HRESULT) -> WinResult<()> {
        self.result.lock().unwrap().result = result;
        Ok(())
    }
    fn PreDeleteItem(&self, dwflags: u32, psiitem: Ref<IShellItem>) -> WinResult<()> {
        let error = match target_identity(&self.path, &self.root) {
            Ok(identity) if identity == self.expected_identity => return Ok(()),
            Ok(_) => "The selected item changed after selection. Refresh and try again.".into(),
            Err(error) => error,
        };
        self.result.lock().unwrap().error = error.clone();
        Err(Error::new(HRESULT(0x80004004u32 as i32), error))
    }
    fn PostDeleteItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        hrdelete: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrrename: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrmove: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        dwflags: u32,
        psiitem: Ref<IShellItem>,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        hrcopy: HRESULT,
        psinewlycreated: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PreNewItem(
        &self,
        dwflags: u32,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
    ) -> WinResult<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        dwflags: u32,
        psidestinationfolder: Ref<IShellItem>,
        psznewname: &PCWSTR,
        psztemplatename: &PCWSTR,
        dwfileattributes: u32,
        hrnew: HRESULT,
        psinewitem: Ref<IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn UpdateProgress(&self, iworktotal: u32, iworksofar: u32) -> WinResult<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> WinResult<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> WinResult<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> WinResult<()> {
        Ok(())
    }
}
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn delete(path: &Path, root: &Path, identity: Identity, owner: i64) -> Result<(), DeleteError> {
    if target_identity(path, root).map_err(DeleteError::Failure)? != identity {
        return Err(DeleteError::Failure(
            "The selected item changed after selection. Refresh and try again.".into(),
        ));
    }
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| DeleteError::Failure(e.to_string()))?;
        let _com = Com;
        let outcome = Arc::new(Mutex::new(OperationResult::default()));
        let sink: IFileOperationProgressSink = Sink {
            path: path.to_path_buf(),
            root: root.to_path_buf(),
            expected_identity: identity,
            result: Arc::clone(&outcome),
        }
        .into();
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| DeleteError::Failure(e.to_string()))?;
        operation
            .SetOwnerWindow(HWND(owner as *mut _))
            .map_err(|e| DeleteError::Failure(e.to_string()))?;
        operation
            .SetOperationFlags(FILEOPERATION_FLAGS(
                FOF_ALLOWUNDO.0 | FOF_WANTNUKEWARNING.0 | 0x20000000,
            ))
            .map_err(|e| DeleteError::Failure(e.to_string()))?;
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide(path).as_ptr()), None)
            .map_err(|e| DeleteError::Failure(e.to_string()))?;
        let cookie = operation
            .Advise(&sink)
            .map_err(|e| DeleteError::Failure(e.to_string()))?;
        let result = operation
            .DeleteItem(&item, None)
            .and_then(|_| operation.PerformOperations());
        let aborted = operation
            .GetAnyOperationsAborted()
            .is_ok_and(|v| v.as_bool());
        let _ = operation.Unadvise(cookie);
        let state = outcome.lock().unwrap();
        if !state.error.is_empty() {
            return Err(DeleteError::Failure(state.error.clone()));
        }
        if aborted {
            return Err(DeleteError::Cancelled);
        }
        let result = result.and_then(|_| state.result.ok());
        if let Err(error) = result {
            if matches!(error.code().0 as u32, 0x80004004 | 0x800704c7 | 0x80270000) {
                return Err(DeleteError::Cancelled);
            }
            return Err(DeleteError::Failure(error.to_string()));
        }
        if fs::symlink_metadata(path).is_ok() {
            return Err(DeleteError::Failure(
                "Windows did not remove the selected item. Refresh the scan before trying again."
                    .into(),
            ));
        }
        Ok(())
    }
}

pub fn reduced_motion() -> bool {
    unsafe {
        let mut enabled = 1i32;
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut i32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && enabled == 0
    }
}
