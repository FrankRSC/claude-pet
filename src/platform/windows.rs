use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Console::{
    AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::{OpenProcess, GetExitCodeProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winit::platform::windows::WindowAttributesExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes};

use crate::geom::Rect;

pub fn window_attributes(attrs: WindowAttributes) -> WindowAttributes {
    attrs.with_skip_taskbar(true).with_undecorated_shadow(false)
}

/// Ventana "layered": transparencia por píxel con UpdateLayeredWindow. Los
/// píxeles con alfa 0 dejan pasar los clics a lo que está detrás.
pub struct Surface {
    hwnd: HWND,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    w: i32,
    h: i32,
}

/// El procedimiento de ventana original de winit (el mismo para todas sus ventanas).
static WINIT_PROC: AtomicIsize = AtomicIsize::new(0);

/// winit responde HTNOWHERE a WM_NCHITTEST en estas ventanas sin bordes, así que
/// Windows nunca les manda clics. Decimos que todo es área cliente; los píxeles
/// transparentes ya los descarta el sistema antes, por ser ventana layered.
unsafe extern "system" fn pet_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return HTCLIENT as LRESULT;
    }
    let prev: WNDPROC = std::mem::transmute(WINIT_PROC.load(Ordering::Relaxed));
    CallWindowProcW(prev, hwnd, msg, wparam, lparam)
}

impl Surface {
    pub fn new(window: &Window, w: u32, h: u32) -> Surface {
        let hwnd = match window.window_handle().expect("sin handle").as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            _ => unreachable!(),
        };
        unsafe {
            let ours = pet_proc as *const () as isize;
            let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours);
            if prev != ours {
                WINIT_PROC.store(prev, Ordering::Relaxed);
            }
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | extra as isize);

            let screen = GetDC(null_mut());
            let mem_dc = CreateCompatibleDC(screen);
            ReleaseDC(null_mut(), screen);

            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w as i32;
            info.bmiHeader.biHeight = -(h as i32); // de arriba hacia abajo
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(mem_dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            let old = SelectObject(mem_dc, bitmap);
            Surface { hwnd, mem_dc, bitmap, old, bits: bits as *mut u32, w: w as i32, h: h as i32 }
        }
    }

    /// Pinta el buffer y mueve la ventana a (x, y) en una sola llamada.
    pub fn move_to(&self, _window: &Window, x: i32, y: i32) {
        unsafe {
            SetWindowPos(self.hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    pub fn present(&mut self, _window: &Window, buf: &[u32]) {
        if self.bits.is_null() {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buf.as_ptr(), self.bits, (self.w * self.h) as usize);
            GdiFlush();
            let size = SIZE { cx: self.w, cy: self.h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let screen = GetDC(null_mut());
            // Sin posición de destino: la ventana se queda donde la dejó `move_to`.
            UpdateLayeredWindow(self.hwnd, screen, null(), &size, self.mem_dc, &src, 0, &blend, ULW_ALPHA);
            ReleaseDC(null_mut(), screen);
        }
    }

    /// Mostrar sin robar el foco a la ventana en la que estás trabajando.
    pub fn show(&self, _window: &Window, visible: bool) {
        unsafe {
            ShowWindow(self.hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
            if visible {
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem_dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.mem_dc);
        }
    }
}

unsafe extern "system" fn collect_monitor(mon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let out = &mut *(data as *mut Vec<Rect>);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(mon, &mut info) != 0 {
        let r = info.rcWork; // sin la barra de tareas
        out.push(Rect { l: r.left as f32, t: r.top as f32, r: r.right as f32, b: r.bottom as f32 });
    }
    1
}

/// Áreas de trabajo de cada monitor; la principal primero.
pub fn work_areas() -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    unsafe {
        EnumDisplayMonitors(null_mut(), std::ptr::null(), Some(collect_monitor), &mut out as *mut _ as LPARAM);
    }
    // El monitor principal es el que contiene (0,0).
    if let Some(i) = out.iter().position(|a| a.contains(0.0, 0.0)) {
        out.swap(0, i);
    }
    out
}

unsafe extern "system" fn collect_window(hwnd: HWND, data: LPARAM) -> BOOL {
    let out = &mut *(data as *mut Vec<(u64, Rect)>);
    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 || GetWindowTextLengthW(hwnd) == 0 {
        return 1;
    }
    if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
        return 1;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == std::process::id() {
        return 1;
    }
    // Ventanas "ocultas" por DWM (apps UWP suspendidas, otros escritorios virtuales).
    let mut cloaked = 0u32;
    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as *mut c_void, 4);
    if cloaked != 0 {
        return 1;
    }
    let mut class = [0u16; 64];
    let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) as usize;
    let class = String::from_utf16_lossy(&class[..n]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return 1;
    }
    // El marco visible, sin los bordes invisibles de redimensionar.
    let mut r: RECT = std::mem::zeroed();
    let ok = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS as u32,
        &mut r as *mut _ as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    ) == 0;
    if !ok {
        GetWindowRect(hwnd, &mut r);
    }
    out.push((hwnd as u64, Rect { l: r.left as f32, t: r.top as f32, r: r.right as f32, b: r.bottom as f32 }));
    1
}

/// Ventanas normales visibles, de la de más adelante a la de más atrás.
pub fn windows() -> Vec<(u64, Rect)> {
    let mut out = Vec::new();
    unsafe {
        EnumWindows(Some(collect_window), &mut out as *mut _ as LPARAM);
    }
    out
}

pub fn cursor_pos() -> (f32, f32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x as f32, p.y as f32)
}

pub fn left_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}

pub fn alt_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0 }
}

pub fn shift_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0 }
}

pub fn low_power() -> bool {
    false
}

pub fn pid_alive(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE as u32
    }
}

fn process_table() -> HashMap<u32, (u32, String)> {
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            map.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    map
}

pub fn find_agent_pid(names: &[&str]) -> Option<u32> {
    let table = process_table();
    super::find_ancestor(std::process::id(), |pid| table.get(&pid).cloned(), names)
}

/// Procesos `name.exe`. Windows no da argv ni carpeta sin leer la memoria del
/// proceso, así que van vacíos (y el descubrimiento de sesiones no aplica).
pub fn processes_named(name: &str) -> Vec<super::ProcInfo> {
    process_table()
        .into_iter()
        .filter(|(_, (_, exe))| exe.to_lowercase().trim_end_matches(".exe") == name)
        .map(|(pid, _)| super::ProcInfo { pid, args: Vec::new(), cwd: None, started: 0 })
        .collect()
}

unsafe extern "system" fn first_window_of(hwnd: HWND, data: LPARAM) -> BOOL {
    let (pid, found) = &mut *(data as *mut (u32, HWND));
    let mut owner = 0u32;
    GetWindowThreadProcessId(hwnd, &mut owner);
    if owner == *pid && IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
        *found = hwnd;
        return 0;
    }
    1
}

fn window_of(pid: u32) -> Option<HWND> {
    let mut data: (u32, HWND) = (pid, null_mut());
    unsafe { EnumWindows(Some(first_window_of), &mut data as *mut _ as LPARAM) };
    (!data.1.is_null()).then_some(data.1)
}

/// Trae al frente la ventana de la sesión del proceso `pid`: sube por sus
/// padres (y la consola `conhost`/`OpenConsole` de cada uno) hasta uno con
/// ventana visible, sin llegar al Explorador. Devuelve el ejecutable de esa
/// ventana, o None si no hay. A mejor esfuerzo: Windows no deja elegir la
/// pestaña de Windows Terminal desde fuera.
pub fn focus_session(pid: u32, _cwd: &str) -> Option<String> {
    let table = process_table();
    let mut p = pid;
    for _ in 0..24 {
        let (ppid, name) = table.get(&p)?.clone();
        if name.eq_ignore_ascii_case("explorer.exe") {
            return None;
        }
        let consoles = table
            .iter()
            .filter(|(_, (parent, n))| *parent == p && (n.eq_ignore_ascii_case("conhost.exe") || n.eq_ignore_ascii_case("OpenConsole.exe")))
            .map(|(c, _)| *c);
        for candidate in std::iter::once(p).chain(consoles) {
            if let Some(hwnd) = window_of(candidate) {
                unsafe {
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    }
                    SetForegroundWindow(hwnd);
                }
                return Some(table.get(&candidate).map_or(name, |(_, n)| n.clone()));
            }
        }
        if ppid == 0 || ppid == p {
            return None;
        }
        p = ppid;
    }
    None
}

/// El WAV que está tocando: PlaySound lee de esta memoria mientras suena.
static PLAYING: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// Toca un WAV en memoria sin bloquear (corta el anterior). Devuelve si empezó.
pub fn play_wav(wav: Vec<u8>) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    let mut current = PLAYING.lock().unwrap();
    unsafe {
        // Primero se corta el anterior: si no, su memoria se liberaría sonando.
        PlaySoundW(null(), null_mut(), 0);
        *current = wav;
        PlaySoundW(current.as_ptr() as *const u16, null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) != 0
    }
}

/// Toca un sonido del sistema (los del Panel de control > Sonido).
pub fn play_system(sound: super::SystemSound) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
    let alias = wide(match sound {
        super::SystemSound::Pop => "SystemNotification",
        super::SystemSound::Done => "SystemAsterisk",
        super::SystemSound::Alert => "SystemExclamation",
    });
    let mut current = PLAYING.lock().unwrap();
    unsafe {
        PlaySoundW(null(), null_mut(), 0);
        current.clear();
        PlaySoundW(alias.as_ptr(), null_mut(), SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
    }
}

pub fn spawn_detached(exe: &Path, args: &[&str]) {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let spawn = |flags: u32| {
        Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // std hereda todos los handles heredables: si el hijo se queda con nuestro
    // stdout (un pipe de Claude Code), quien nos llamó esperaría para siempre.
    unsafe {
        for std in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(std);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
    // Salir del "job" de Claude Code para que no nos cierre junto con el hook.
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if spawn(base | CREATE_BREAKAWAY_FROM_JOB).is_err() {
        let _ = spawn(base);
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn set_autostart(exe: Option<&Path>) {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("ClaudePet");
    unsafe {
        match exe {
            Some(exe) => {
                let value = wide(&format!("\"{}\" run", exe.display()));
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    value.as_ptr() as *const c_void,
                    (value.len() * 2) as u32,
                );
            }
            None => {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
            }
        }
    }
}

/// En release el .exe es de subsistema "windows"; esto recupera la consola
/// de la terminal para que `install` pueda imprimir.
pub fn attach_console() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
