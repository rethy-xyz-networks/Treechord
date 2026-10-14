#![windows_subsystem = "windows"]

mod config;
mod window;

use config::{Bind, Config};
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::io::Write;
use window::{raise_or_run, wide};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::Console::{AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT, RAWINPUTDEVICE,
    RAWINPUTHEADER, RIDEV_INPUTSINK, RID_DEVICE_INFO, RID_INPUT, RIDI_DEVICEINFO, RIM_TYPEHID,
};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_FIRE: u32 = WM_APP + 1;
const WM_TRAY: u32 = WM_APP + 2;
const WM_RELOAD: u32 = WM_APP + 3;

const ID_RELOAD: usize = 1;
const ID_SUSPEND: usize = 2;
const ID_EXIT: usize = 3;

const WINDOW_CLASS: &str = "TreechordWindow";
/// Unassigned virtual key, pressed and released to stop the Start menu opening after Win+key.
const VK_MASK: u16 = 0xE8;

/// Microsoft Natural Ergonomic Keyboard 4000.
const NATURAL_VENDOR: u32 = 0x045E;
const NATURAL_PRODUCT: u32 = 0x00DB;

struct App {
    config: Config,
    config_path: String,
    suspended: bool,
    hwnd: HWND,
    /// Key-down events we swallowed, so their key-up is swallowed too.
    swallowed: HashSet<u32>,
    /// Bind names triggered by the hook, waiting for the message loop.
    pending: VecDeque<String>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn default_config_path() -> String {
    format!("C:\\Users\\{}\\Treechord.json", std::env::var("USERNAME").unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------

/// `buf` filled with `s` (truncated, NUL-terminated). Takes and returns by value because the
/// NOTIFYICONDATAW fields are unaligned in a packed struct on 32-bit Windows.
fn filled<const N: usize>(mut buf: [u16; N], s: &str) -> [u16; N] {
    for (i, c) in s.encode_utf16().take(N - 1).enumerate() {
        buf[i] = c;
    }
    buf
}

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

unsafe fn tray_add(hwnd: HWND, suspended: bool) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    // shell32.dll icon #42, as in the AutoHotkey version.
    let mut small = std::ptr::null_mut();
    ExtractIconExW(wide("shell32.dll").as_ptr(), 41, std::ptr::null_mut(), &mut small, 1);
    nid.hIcon = small;
    nid.szTip = filled(nid.szTip, if suspended { "Treechord (suspended)" } else { "Treechord" });
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn tray_update_tip(hwnd: HWND, suspended: bool) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_TIP;
    nid.szTip = filled(nid.szTip, if suspended { "Treechord (suspended)" } else { "Treechord" });
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn tray_remove(hwnd: HWND) {
    let nid = tray_data(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

fn tray_tip(title: &str, text: &str) {
    let Some(hwnd) = with_app(|a| a.hwnd) else { return };
    unsafe {
        let mut nid = tray_data(hwnd);
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_NONE;
        nid.szInfoTitle = filled(nid.szInfoTitle, title);
        nid.szInfo = filled(nid.szInfo, text);
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

unsafe fn show_menu(hwnd: HWND, suspended: bool) {
    let menu = CreatePopupMenu();
    AppendMenuW(menu, MF_STRING, ID_RELOAD, wide("Reload Configuration").as_ptr());
    let flags = MF_STRING | if suspended { MF_CHECKED } else { 0 };
    AppendMenuW(menu, flags, ID_SUSPEND, wide("Suspend Hotkeys").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, ID_EXIT, wide("Exit").as_ptr());
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// (Re)load the configuration file, creating it if absent. A bad entry or
/// unreadable file disables the hotkeys and reports the problem.
fn load_config() {
    let Some(path) = with_app(|a| a.config_path.clone()) else { return };
    match config::init(&path) {
        Ok(true) => tray_tip("[CREATED CONFIGURATION]", &format!("-> {path} <-")),
        Ok(false) => {}
        Err(e) => tray_tip("[ERROR]", &format!("{path}: {e}")),
    }
    let (suspended, message) = match config::load(&path) {
        Ok(loaded) => {
            let message = loaded
                .errors
                .first()
                .map(|(k, v)| format!("{path}: \"{k}\": {v}."));
            with_app(|a| a.config = loaded.config);
            (message.is_some(), message)
        }
        Err(e) => (true, Some(e)),
    };
    if let Some(m) = message {
        tray_tip("[ERROR]", &m);
    }
    set_suspended(suspended);
}

fn set_suspended(suspended: bool) {
    if let Some(hwnd) = with_app(|a| {
        a.suspended = suspended;
        a.hwnd
    }) {
        unsafe { tray_update_tip(hwnd, suspended) };
    }
}

fn run_bind(name: &str) {
    let bind: Option<Bind> = with_app(|a| a.config.binds.get(name).cloned()).flatten();
    if let Some(bind) = bind {
        if let Err(path) = raise_or_run(&bind) {
            tray_tip("Invalid path", &format!("{path}."));
        }
    }
}

// ---------------------------------------------------------------------------
// Sending keys
// ---------------------------------------------------------------------------

fn key_input(vk: u16, up: bool, extended: bool) -> INPUT {
    let mut flags = 0;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn send_inputs(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

/// Press `mods`, tap `key`, release `mods`.
fn send_chord(mods: &[u16], key: u16, extended: bool) {
    let mut inputs: Vec<INPUT> = mods.iter().map(|&m| key_input(m, false, false)).collect();
    inputs.push(key_input(key, false, extended));
    inputs.push(key_input(key, true, extended));
    inputs.extend(mods.iter().rev().map(|&m| key_input(m, true, false)));
    send_inputs(&inputs);
}

// ---------------------------------------------------------------------------
// Keyboard hook: Win + key chords
// ---------------------------------------------------------------------------

fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

/// Name of the binding for Win + (modifiers) + `vk`, if there is one.
fn bind_name(vk: u32, shift: bool, alt: bool) -> Option<String> {
    if shift && alt {
        return None;
    }
    let prefix = if shift {
        "Shift"
    } else if alt {
        "Alt"
    } else {
        ""
    };
    match vk {
        0x41..=0x5A => {
            let letter = vk as u8 as char;
            // Win+D, Win+R and Win+X are left to Windows.
            if prefix.is_empty() && matches!(letter, 'D' | 'R' | 'X') {
                return None;
            }
            Some(format!("bind{prefix}{letter}"))
        }
        0x0D if !alt => Some(format!("bind{prefix}Enter")),
        _ => None,
    }
}

/// Returns true if the event should be swallowed.
fn handle_key(msg: u32, vk: u32) -> bool {
    let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;

    APP.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false };
        let Some(app) = guard.as_mut() else { return false };

        if is_up {
            return app.swallowed.remove(&vk);
        }
        if !is_down || !(key_down(VK_LWIN) || key_down(VK_RWIN)) {
            return false;
        }
        let ctrl = key_down(VK_CONTROL);
        let shift = key_down(VK_SHIFT);
        let alt = key_down(VK_MENU);

        let swallow = |app: &mut App| {
            app.swallowed.insert(vk);
            // Stop Windows treating the Win release as a lone Win press.
            send_inputs(&[key_input(VK_MASK, false, false), key_input(VK_MASK, true, false)]);
            true
        };

        // Win+Ctrl+R reloads the configuration.
        if ctrl && vk == b'R' as u32 && !shift && !alt {
            unsafe { PostMessageW(app.hwnd, WM_RELOAD, 0, 0) };
            return swallow(app);
        }
        if ctrl || app.suspended {
            return false;
        }
        match bind_name(vk, shift, alt) {
            Some(name) => {
                app.pending.push_back(name);
                unsafe { PostMessageW(app.hwnd, WM_FIRE, 0, 0) };
                swallow(app)
            }
            None => false,
        }
    })
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Our own injected keys (VK_MASK, chords for the Natural 4000) must never re-trigger us.
        // The `test-injected` feature lets automated tests drive the hook with synthetic keys.
        let ours = k.flags & LLKHF_INJECTED != 0 && !(cfg!(feature = "test-injected") && k.vkCode != VK_MASK as u32);
        if !ours && handle_key(wparam as u32, k.vkCode) {
            return 1;
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

// ---------------------------------------------------------------------------
// Natural Keyboard 4000 extra keys (raw HID input)
// ---------------------------------------------------------------------------

unsafe fn register_raw_input(hwnd: HWND) {
    let device = RAWINPUTDEVICE { usUsagePage: 12, usUsage: 1, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    RegisterRawInputDevices(&device, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32);
}

/// Returns the 8-byte report of a Natural 4000 HID event, or None for any other input.
unsafe fn natural_report(lparam: LPARAM) -> Option<[u8; 8]> {
    let handle = lparam as HRAWINPUT;
    let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    GetRawInputData(handle, RID_INPUT, std::ptr::null_mut(), &mut size, header_size);
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if GetRawInputData(handle, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, header_size) == u32::MAX {
        return None;
    }
    let header = &*(buf.as_ptr() as *const RAWINPUTHEADER);
    if header.dwType != RIM_TYPEHID {
        return None;
    }

    let mut info: RID_DEVICE_INFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
    let mut info_size = info.cbSize;
    let got = GetRawInputDeviceInfoW(header.hDevice, RIDI_DEVICEINFO, (&mut info as *mut RID_DEVICE_INFO).cast(), &mut info_size);
    if got == u32::MAX || info.Anonymous.hid.dwVendorId != NATURAL_VENDOR || info.Anonymous.hid.dwProductId != NATURAL_PRODUCT {
        return None;
    }

    // RAWHID: dwSizeHid, dwCount, then the report bytes.
    let hid = &buf[header_size as usize..];
    let size_hid = u32::from_le_bytes(hid.get(0..4)?.try_into().ok()?) as usize;
    let count = u32::from_le_bytes(hid.get(4..8)?.try_into().ok()?) as usize;
    if size_hid * count != 8 {
        return None;
    }
    hid.get(8..16)?.try_into().ok()
}

fn handle_natural_key(report: [u8; 8]) {
    // Strip the modifier byte and the Fn bit, as the AutoHotkey version does.
    let code = u64::from_be_bytes(report) & 0x0FFF_FFFF_FFFE_FF00;
    match code {
        0x0100_0000_0004_0000 => run_bind("bindLaunch1"),
        0x0100_0000_0008_0000 => run_bind("bindLaunch2"),
        0x0100_0000_0010_0000 => run_bind("bindLaunch3"),
        0x0100_0000_0020_0000 => run_bind("bindLaunch4"),
        0x0100_0000_0040_0000 => run_bind("bindLaunch5"),
        // Numpad = -> paste
        0x0100_0067_0000_0000 => send_chord(&[VK_CONTROL], b'V' as u16, false),
        // Numpad ( -> previous virtual desktop, ) -> next
        0x0100_00B6_0000_0000 => send_chord(&[VK_CONTROL, VK_LWIN], VK_LEFT, true),
        0x0100_00B7_0000_0000 => send_chord(&[VK_CONTROL, VK_LWIN], VK_RIGHT, true),
        // Zoom slider
        0x012E_0200_0000_0000 => send_chord(&[VK_CONTROL], VK_OEM_MINUS, false),
        0x012D_0200_0000_0000 => send_chord(&[VK_CONTROL, VK_SHIFT], VK_OEM_PLUS, false),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Window / message loop
// ---------------------------------------------------------------------------

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_FIRE => {
            while let Some(name) = with_app(|a| a.pending.pop_front()).flatten() {
                run_bind(&name);
            }
            0
        }
        WM_RELOAD => {
            load_config();
            0
        }
        WM_TRAY => {
            if (lparam as u32) == WM_RBUTTONUP || (lparam as u32) == WM_CONTEXTMENU {
                let suspended = with_app(|a| a.suspended).unwrap_or(false);
                show_menu(hwnd, suspended);
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                ID_RELOAD => load_config(),
                ID_SUSPEND => {
                    let now = with_app(|a| a.suspended).unwrap_or(false);
                    set_suspended(!now);
                }
                ID_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_INPUT => {
            if let Some(report) = natural_report(lparam) {
                handle_natural_key(report);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            tray_remove(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => {
            if msg == RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) {
                let suspended = with_app(|a| a.suspended).unwrap_or(false);
                tray_add(hwnd, suspended);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

/// Replace a running instance, like AutoHotkey does.
unsafe fn close_previous_instance() {
    let old = FindWindowW(wide(WINDOW_CLASS).as_ptr(), std::ptr::null());
    if !old.is_null() {
        PostMessageW(old, WM_CLOSE, 0, 0);
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if FindWindowW(wide(WINDOW_CLASS).as_ptr(), std::ptr::null()).is_null() {
                break;
            }
        }
    }
}

/// Console output for command-line modes (the binary is a GUI-subsystem program).
fn console_out() -> Box<dyn Write> {
    // Piped/redirected stdout is inherited as-is; otherwise write to the parent's console.
    let inherited = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !inherited.is_null() && inherited as isize != -1 {
        return Box::new(std::io::stdout());
    }
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0 {
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            return Box::new(f);
        }
    }
    Box::new(std::io::stdout())
}

fn usage() -> &'static str {
    "Treechord - raise-or-run launcher\n\
     \n\
     USAGE: treechord [CONFIG.json]\n\
     \x20      treechord --check [CONFIG.json]      validate and list the bindings\n\
     \x20      treechord --trigger NAME [CONFIG.json]  run one binding (e.g. bindA) and exit\n\
     \n\
     Default configuration: C:\\Users\\<user>\\Treechord.json (created if missing)\n"
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut check = false;
    let mut trigger: Option<String> = None;
    let mut config_path = None;
    while !args.is_empty() {
        let a = args.remove(0);
        match a.as_str() {
            "--check" => check = true,
            "--trigger" if !args.is_empty() => trigger = Some(args.remove(0)),
            "-h" | "--help" => {
                let _ = console_out().write_all(usage().as_bytes());
                return;
            }
            _ => config_path = Some(a),
        }
    }
    let config_path = config_path.unwrap_or_else(default_config_path);

    if check || trigger.is_some() {
        let mut out = console_out();
        let loaded = match config::load(&config_path) {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(out, "ERROR: {e}");
                std::process::exit(1);
            }
        };
        for (k, v) in &loaded.errors {
            let _ = writeln!(out, "ERROR: unknown or invalid entry \"{k}\": {v}");
        }
        if let Some(name) = trigger {
            match loaded.config.binds.get(&name) {
                Some(b) => {
                    let _ = writeln!(out, "{name}: target={:?} path={:?}", b.target, b.path);
                    if let Err(p) = raise_or_run(b) {
                        let _ = writeln!(out, "Invalid path: {p}");
                        std::process::exit(1);
                    }
                }
                None => {
                    let _ = writeln!(out, "no such binding: {name}");
                    std::process::exit(1);
                }
            }
            return;
        }
        let mut names: Vec<_> = loaded.config.binds.iter().collect();
        names.sort_by_key(|(k, _)| config::valid_keys().iter().position(|v| v == *k));
        for (name, b) in names.iter().filter(|(_, b)| !b.is_empty()) {
            let _ = writeln!(out, "{name:<16} {:<24} {}", b.target, b.path);
        }
        let _ = writeln!(out, "{} bindings set, {} errors", names.iter().filter(|(_, b)| !b.is_empty()).count(), loaded.errors.len());
        std::process::exit(if loaded.errors.is_empty() { 0 } else { 1 });
    }

    unsafe {
        close_previous_instance();

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = wide(WINDOW_CLASS);
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinstance;
        wc.lpszClassName = class_name.as_ptr();
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("Treechord").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                config: Config::default(),
                config_path,
                suspended: false,
                hwnd,
                swallowed: HashSet::new(),
                pending: VecDeque::new(),
            })
        });

        tray_add(hwnd, false);
        register_raw_input(hwnd);
        load_config();

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinstance, 0);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
    }
}
