//! Running processes on Windows, read from a Toolhelp snapshot instead of starting
//! `tasklist` on every poll while a game runs.

use std::mem::size_of;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    },
};

/// One running process: its id and executable file name, as `tasklist` shows them.
pub(super) struct RunningProcess {
    pub pid: u32,
    pub exe: String,
}

/// Every running process, or `None` when the snapshot cannot be taken.
pub(super) fn running_processes() -> Option<Vec<RunningProcess>> {
    // SAFETY: a plain call with valid flags; the handle is checked before any use.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return None;
    }
    let snapshot = Snapshot(handle);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();
    // SAFETY: `snapshot` holds a valid snapshot handle until it drops, after this loop, and
    // `entry` is a PROCESSENTRY32W whose `dwSize` is set, which the call fills in.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let name = &entry.szExeFile;
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        processes.push(RunningProcess {
            pid: entry.th32ProcessID,
            exe: String::from_utf16_lossy(&name[..len]),
        });
        // SAFETY: as for `Process32FirstW`.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    Some(processes)
}

/// Closes the snapshot handle exactly once.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateToolhelp32Snapshot`, is valid, and is closed
        // only here.
        unsafe { CloseHandle(self.0) };
    }
}
