//! Raise-or-run: focus an existing window of a program, otherwise launch it.

use crate::config::Bind;
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows_sys::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Win {
    hwnd: HWND,
    exe: String,
    class: String,
}

unsafe fn exe_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return String::new();
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
    CloseHandle(process);
    if ok == 0 {
        return String::new();
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    full.rsplit('\\').next().unwrap_or("").to_lowercase()
}

unsafe fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n.max(0) as usize]).to_lowercase()
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let list = &mut *(lparam as *mut Vec<HWND>);
    if IsWindowVisible(hwnd) != 0 {
        list.push(hwnd);
    }
    1
}

/// Visible top-level windows, topmost first.
unsafe fn visible_windows() -> Vec<Win> {
    let mut handles: Vec<HWND> = Vec::new();
    EnumWindows(Some(collect), &mut handles as *mut _ as LPARAM);
    handles
        .into_iter()
        .map(|hwnd| Win { hwnd, exe: exe_name(hwnd), class: class_name(hwnd) })
        .collect()
}

unsafe fn activate(hwnd: HWND) {
    if IsIconic(hwnd) != 0 {
        ShowWindow(hwnd, SW_RESTORE);
    }
    // Borrow the foreground thread's input state so SetForegroundWindow is allowed.
    let foreground = GetForegroundWindow();
    let fg_thread = if foreground.is_null() {
        0
    } else {
        GetWindowThreadProcessId(foreground, std::ptr::null_mut())
    };
    let this_thread = GetCurrentThreadId();
    let attached = fg_thread != 0 && fg_thread != this_thread && AttachThreadInput(this_thread, fg_thread, 1) != 0;
    BringWindowToTop(hwnd);
    SetForegroundWindow(hwnd);
    if attached {
        AttachThreadInput(this_thread, fg_thread, 0);
    }
}

/// Returns `Err(path)` if the program could not be launched.
pub fn raise_or_run(bind: &Bind) -> Result<(), String> {
    if bind.is_empty() {
        return Ok(());
    }
    unsafe {
        if !bind.target.is_empty() {
            let target = bind.target.to_lowercase();
            let wins = visible_windows();
            let matching: Vec<&Win> = wins.iter().filter(|w| w.exe == target || w.class == target).collect();
            if !matching.is_empty() {
                let foreground = GetForegroundWindow();
                // Already in this program: cycle to its bottom-most window.
                let pick = if matching.iter().any(|w| w.hwnd == foreground) {
                    matching.last()
                } else {
                    matching.first()
                };
                activate(pick.unwrap().hwnd);
                return Ok(());
            }
        }
        if bind.path.is_empty() {
            return Ok(());
        }
        let path = bind.path.replace('/', "\\");
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            wide("open").as_ptr(),
            wide(&path).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        if (result as isize) <= 32 {
            return Err(bind.path.clone());
        }
    }
    Ok(())
}
