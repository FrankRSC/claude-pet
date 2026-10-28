//! Backend de macOS: ventana transparente de winit + CALayer con un CGImage.
//! Coordenadas: winit usa píxeles físicos con origen arriba a la izquierda, así
//! que convertimos desde los puntos de AppKit (origen abajo a la izquierda).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use core_graphics::base::{kCGBitmapByteOrder32Little, kCGImageAlphaPremultipliedFirst, kCGRenderingIntentDefault};
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
use foreign_types::ForeignType;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect};
use std::os::unix::process::CommandExt;
use winit::dpi::PhysicalPosition;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes};

use crate::geom::Rect;

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}

pub fn window_attributes(attrs: WindowAttributes) -> WindowAttributes {
    attrs.with_transparent(true)
}

pub struct Surface {
    layer: *mut AnyObject,
    colorspace: CGColorSpace,
    w: usize,
    h: usize,
}

impl Surface {
    pub fn new(window: &Window, w: u32, h: u32) -> Surface {
        let view = match window.window_handle().expect("sin handle").as_raw() {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as *mut AnyObject,
            _ => unreachable!(),
        };
        unsafe {
            let _: () = msg_send![view, setWantsLayer: true];
            let layer: *mut AnyObject = msg_send![view, layer];
            let ns_window: *mut AnyObject = msg_send![view, window];
            let _: () = msg_send![ns_window, setHasShadow: false];
            // Ventana e imagen en sRGB: así Core Animation no tiene que convertir
            // cada cuadro al espacio de color de la pantalla.
            let srgb: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
            let _: () = msg_send![ns_window, setColorSpace: srgb];
            // canJoinAllSpaces | stationary | ignoresCycle: visible en todos los escritorios.
            let _: () = msg_send![ns_window, setCollectionBehavior: (1usize << 0) | (1 << 4) | (1 << 6)];
            let colorspace = CGColorSpace::create_with_name(core_graphics::color_space::kCGColorSpaceSRGB)
                .unwrap_or_else(CGColorSpace::create_device_rgb);
            Surface { layer, colorspace, w: w as usize, h: h as usize }
        }
    }

    pub fn move_to(&self, window: &Window, x: i32, y: i32) {
        window.set_outer_position(PhysicalPosition::new(x, y));
    }

    pub fn present(&mut self, _window: &Window, buf: &[u32]) {
        // BGRA en little-endian: los u32 ya tienen el orden de bytes que pide CGImage.
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len() * 4) }.to_vec();
        let provider = CGDataProvider::from_buffer(Arc::new(bytes));
        let image = CGImage::new(
            self.w,
            self.h,
            8,
            32,
            self.w * 4,
            &self.colorspace,
            kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little,
            &provider,
            false,
            kCGRenderingIntentDefault,
        );
        unsafe {
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions: true];
            let _: () = msg_send![self.layer, setContents: image.as_ptr() as *mut AnyObject];
            let _: () = msg_send![class!(CATransaction), commit];
        }
    }

    pub fn show(&self, window: &Window, visible: bool) {
        window.set_visible(visible);
    }
}

unsafe fn screens() -> Vec<(NSRect, NSRect, f64)> {
    let list: *mut AnyObject = msg_send![class!(NSScreen), screens];
    let count: usize = msg_send![list, count];
    (0..count)
        .map(|i| {
            let s: *mut AnyObject = msg_send![list, objectAtIndex: i];
            let frame: NSRect = msg_send![s, frame];
            let visible: NSRect = msg_send![s, visibleFrame];
            let scale: f64 = msg_send![s, backingScaleFactor];
            (frame, visible, scale)
        })
        .collect()
}

/// Alto y escala de la pantalla principal. Se usa en cada cuadro (cursor,
/// ventanas), así que se guarda y solo se recalcula en `work_areas`.
static MAIN_SCREEN: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);

fn read_main_screen() -> (f64, f64) {
    let v = unsafe { screens().first().map(|(f, _, s)| (f.size.height, *s)).unwrap_or((1080.0, 1.0)) };
    *MAIN_SCREEN.lock().unwrap() = Some(v);
    v
}

fn main_height_and_scale() -> (f64, f64) {
    let cached = *MAIN_SCREEN.lock().unwrap();
    cached.unwrap_or_else(read_main_screen)
}

/// Áreas visibles (sin barra de menú ni Dock) en píxeles físicos.
pub fn work_areas() -> Vec<Rect> {
    let (main_h, _) = read_main_screen();
    unsafe {
        screens()
            .into_iter()
            .map(|(_, v, scale)| {
                let top = main_h - (v.origin.y + v.size.height);
                Rect {
                    l: (v.origin.x * scale) as f32,
                    t: (top * scale) as f32,
                    r: ((v.origin.x + v.size.width) * scale) as f32,
                    b: ((top + v.size.height) * scale) as f32,
                }
            })
            .collect()
    }
}

/// Ventanas normales visibles (capa 0), de la de más adelante a la de más atrás.
pub fn windows() -> Vec<(u64, Rect)> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::geometry::CGRect;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    };

    let (_, scale) = main_height_and_scale();
    let Some(list) = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return Vec::new();
    };
    let me = std::process::id() as i64;
    let mut out = Vec::new();
    for item in list.iter() {
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let num = |k: &'static str| {
            dict.find(&CFString::from_static_string(k))
                .and_then(|v| v.downcast::<CFNumber>())
                .and_then(|n| n.to_i64())
        };
        if num("kCGWindowLayer") != Some(0) || num("kCGWindowOwnerPID") == Some(me) {
            continue;
        }
        let Some(id) = num("kCGWindowNumber") else { continue };
        let Some(bounds) = dict
            .find(&CFString::from_static_string("kCGWindowBounds"))
            .and_then(|v| v.downcast::<CFDictionary>())
        else {
            continue;
        };
        let Some(r) = CGRect::from_dict_representation(&bounds) else { continue };
        // CGWindow ya usa origen arriba a la izquierda (en puntos).
        let s = scale;
        out.push((
            id as u64,
            Rect {
                l: (r.origin.x * s) as f32,
                t: (r.origin.y * s) as f32,
                r: ((r.origin.x + r.size.width) * s) as f32,
                b: ((r.origin.y + r.size.height) * s) as f32,
            },
        ));
    }
    out
}

pub fn cursor_pos() -> (f32, f32) {
    let (main_h, scale) = main_height_and_scale();
    let p: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    ((p.x * scale) as f32, ((main_h - p.y) * scale) as f32)
}

pub fn left_button_down() -> bool {
    let buttons: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
    buttons & 1 != 0
}

/// ⌥ Option presionada: con trackpad no hay clic central.
pub fn alt_down() -> bool {
    const OPTION: usize = 1 << 19; // NSEventModifierFlagOption
    let flags: usize = unsafe { msg_send![class!(NSEvent), modifierFlags] };
    flags & OPTION != 0
}

/// ⇧ Shift presionada.
pub fn shift_down() -> bool {
    const SHIFT: usize = 1 << 17; // NSEventModifierFlagShift
    let flags: usize = unsafe { msg_send![class!(NSEvent), modifierFlags] };
    flags & SHIFT != 0
}

/// Modo de bajo consumo de macOS (el de la batería) activado.
pub fn low_power() -> bool {
    unsafe {
        let info: *mut AnyObject = msg_send![class!(NSProcessInfo), processInfo];
        msg_send![info, isLowPowerModeEnabled]
    }
}

pub fn pid_alive(pid: u32) -> bool {
    // Señal 0: solo pregunta si existe. EPERM = existe pero es de otro usuario.
    let ok = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    ok || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    (got == size).then_some(info)
}

/// Texto C de un arreglo de tamaño fijo, sin pasarse si no trae el NUL final.
fn c_text(raw: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// (ppid, nombre) de un proceso, sin lanzar `ps`. Usa la variante corta de
/// proc_pidinfo porque la completa falla con procesos de otro usuario, como
/// el `login` (de root) que hay entre una terminal y su shell.
fn parent_of(pid: u32) -> Option<(u32, String)> {
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDT_SHORTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    (got == size).then(|| (info.pbsi_ppid, c_text(&info.pbsi_comm)))
}

pub fn find_agent_pid(names: &[&str]) -> Option<u32> {
    super::find_ancestor(std::process::id(), parent_of, names)
}

fn proc_cwd(pid: u32) -> Option<String> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDVNODEPATHINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    if got != size {
        return None;
    }
    Some(c_text(info.pvi_cdir.vip_path.as_flattened())).filter(|s| !s.is_empty())
}

/// argv de un proceso (KERN_PROCARGS2). Solo funciona con procesos del mismo usuario.
fn proc_args(pid: u32) -> Vec<String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = 0;
    let null = std::ptr::null_mut();
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, null, &mut size, null, 0) } != 0 || size == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; size];
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr() as *mut libc::c_void, &mut size, null, 0) } != 0 {
        return Vec::new();
    }
    buf.truncate(size);
    parse_procargs(&buf)
}

/// Formato de KERN_PROCARGS2: argc (i32), ruta del ejecutable, NULs de
/// relleno y luego argc cadenas separadas por NUL (después viene el entorno).
fn parse_procargs(buf: &[u8]) -> Vec<String> {
    let Some(argc) = buf.get(..4).map(|b| i32::from_ne_bytes([b[0], b[1], b[2], b[3]]).max(0) as usize) else {
        return Vec::new();
    };
    let rest = &buf[4..];
    let Some(mut i) = rest.iter().position(|&b| b == 0) else { return Vec::new() };
    while rest.get(i) == Some(&0) {
        i += 1;
    }
    rest[i..].split(|&b| b == 0).take(argc).map(|s| String::from_utf8_lossy(s).into_owned()).collect()
}

/// Procesos que se llaman exactamente `name`, con su argv, carpeta y arranque.
pub fn processes_named(name: &str) -> Vec<super::ProcInfo> {
    let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if n <= 0 {
        return Vec::new();
    }
    let mut pids = vec![0 as libc::c_int; n as usize + 64];
    let bytes = (pids.len() * std::mem::size_of::<libc::c_int>()) as libc::c_int;
    let got = unsafe { libc::proc_listallpids(pids.as_mut_ptr() as *mut libc::c_void, bytes) };
    pids.truncate(got.max(0) as usize);
    pids.into_iter()
        .filter(|&p| p > 0)
        .filter_map(|p| {
            let pid = p as u32;
            let info = bsd_info(pid)?;
            (c_text(&info.pbi_comm) == name).then(|| super::ProcInfo {
                pid,
                args: proc_args(pid),
                cwd: proc_cwd(pid),
                started: info.pbi_start_tvsec,
            })
        })
        .collect()
}

/// La terminal de un proceso ("/dev/ttys003"), si tiene.
fn tty_of(pid: u32) -> Option<String> {
    let dev = bsd_info(pid)?.e_tdev;
    if dev == u32::MAX || dev == 0 {
        return None;
    }
    let name = unsafe { libc::devname(dev as libc::dev_t, libc::S_IFCHR) };
    if name.is_null() {
        return None;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned();
    (!name.is_empty() && name != "??").then(|| format!("/dev/{name}"))
}

/// La primera app con ventanas (política Regular) subiendo desde `pid`:
/// (pid de la app, bundle id si tiene).
fn gui_ancestor(pid: u32) -> Option<(u32, Option<String>)> {
    let mut p = pid;
    for _ in 0..24 {
        let app: *mut AnyObject =
            unsafe { msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: p as libc::pid_t] };
        if !app.is_null() {
            let policy: isize = unsafe { msg_send![app, activationPolicy] };
            if policy == 0 {
                let id: *mut AnyObject = unsafe { msg_send![app, bundleIdentifier] };
                let bundle = (!id.is_null()).then(|| {
                    let utf8: *const libc::c_char = unsafe { msg_send![id, UTF8String] };
                    unsafe { std::ffi::CStr::from_ptr(utf8) }.to_string_lossy().into_owned()
                });
                return Some((p, bundle));
            }
        }
        let (ppid, _) = parent_of(p)?;
        if ppid <= 1 || ppid == p {
            return None;
        }
        p = ppid;
    }
    None
}

/// Editores tipo VS Code: `open -b <id> <carpeta>` enfoca la ventana de esa carpeta.
const EDITORS: [&str; 5] = [
    "com.microsoft.VSCode",
    "com.microsoft.VSCodeInsiders",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.exafunction.windsurf",
    "com.vscodium",
];

/// AppleScript que selecciona la pestaña (o sesión de iTerm2) con esa tty.
fn select_tab_script(bundle: &str, tty: &str) -> Option<String> {
    let body = match bundle {
        "com.apple.Terminal" => format!(
            r#"tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is "{tty}" then
        set selected of t to true
        set index of w to 1
        activate
        return "ok"
      end if
    end repeat
  end repeat
end tell
return "no""#
        ),
        "com.googlecode.iterm2" => format!(
            r#"tell application "iTerm2"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if tty of s is "{tty}" then
          select w
          select t
          select s
          activate
          return "ok"
        end if
      end repeat
    end repeat
  end repeat
end tell
return "no""#
        ),
        _ => return None,
    };
    Some(body)
}

/// La carpeta que el editor tiene abierta, probablemente: la raíz del repo
/// (la que tiene .git) o, si no hay, la carpeta misma.
fn workspace_of(cwd: &str) -> String {
    let start = Path::new(cwd);
    start
        .ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(start)
        .display()
        .to_string()
}

/// Corre un comando y dice si salió bien (un AppleScript que devuelve "no"
/// cuenta como fallo). Lo que falle queda en el log de depuración.
fn run_ok(cmd: &str, args: &[&str]) -> bool {
    match Command::new(cmd).args(args).stdin(Stdio::null()).output() {
        Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() != "no" => true,
        Ok(o) => {
            let why = String::from_utf8_lossy(&o.stderr);
            let out = String::from_utf8_lossy(&o.stdout);
            crate::app::debug_log(format_args!("{cmd} falló: {} {}", out.trim(), why.trim()));
            false
        }
        Err(e) => {
            crate::app::debug_log(format_args!("{cmd} no corrió: {e}"));
            false
        }
    }
}

/// Trae al frente la ventana de la sesión del proceso `pid`: la pestaña exacta
/// en Terminal e iTerm2 (macOS pide permiso de Automatización la primera vez),
/// la carpeta en editores tipo VS Code y, si no, la app. Devuelve la app a la
/// que va (su bundle id), o None si la sesión no tiene ventana; el cambio de
/// foco ocurre en otro hilo.
pub fn focus_session(pid: u32, cwd: &str) -> Option<String> {
    let (app_pid, bundle) = gui_ancestor(pid)?;
    let Some(bundle) = bundle else {
        let app: *mut AnyObject =
            unsafe { msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: app_pid as libc::pid_t] };
        let _: bool = unsafe { msg_send![app, activateWithOptions: 1usize] };
        return Some(format!("pid {app_pid}"));
    };
    let script = tty_of(pid).and_then(|tty| select_tab_script(&bundle, &tty));
    let folder = (EDITORS.contains(&bundle.as_str()) && !cwd.is_empty()).then(|| workspace_of(cwd));
    let id = bundle.clone();
    std::thread::spawn(move || {
        let done = match (&script, &folder) {
            (Some(s), _) => run_ok("/usr/bin/osascript", &["-e", s]),
            (None, Some(dir)) => run_ok("/usr/bin/open", &["-b", &id, dir]),
            _ => false,
        };
        if !done {
            run_ok("/usr/bin/open", &["-b", &id]);
        }
    });
    Some(bundle)
}

thread_local! {
    /// Sonidos que siguen tocando; se sueltan cuando terminan. Solo se toca
    /// desde el hilo principal.
    static PLAYING: std::cell::RefCell<Vec<objc2::rc::Retained<AnyObject>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Toca un WAV en memoria sin bloquear. Devuelve si empezó a sonar.
pub fn play_wav(wav: Vec<u8>) -> bool {
    objc2::rc::autoreleasepool(|_| unsafe {
        PLAYING.with_borrow_mut(|list| list.retain(|s| msg_send![&**s, isPlaying]));
        let data: *mut AnyObject =
            msg_send![class!(NSData), dataWithBytes: wav.as_ptr() as *const libc::c_void, length: wav.len()];
        let alloc: objc2::rc::Allocated<AnyObject> = msg_send![class!(NSSound), alloc];
        let sound: Option<objc2::rc::Retained<AnyObject>> = msg_send![alloc, initWithData: data];
        let Some(sound) = sound else { return false };
        let started: bool = msg_send![&*sound, play];
        if started {
            PLAYING.with_borrow_mut(|list| list.push(sound));
        }
        started
    })
}

/// Toca un sonido del sistema (los de Preferencias > Sonido).
pub fn play_system(sound: super::SystemSound) {
    let name = match sound {
        super::SystemSound::Pop => c"Pop",
        super::SystemSound::Done => c"Glass",
        super::SystemSound::Alert => c"Funk",
    };
    objc2::rc::autoreleasepool(|_| unsafe {
        let ns: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
        let sound: *mut AnyObject = msg_send![class!(NSSound), soundNamed: ns];
        if sound.is_null() {
            return;
        }
        let _: bool = msg_send![sound, stop];
        let _: () = msg_send![sound, setVolume: 0.6f32];
        let _: bool = msg_send![sound, play];
    });
}

pub fn spawn_detached(exe: &Path, args: &[&str]) {
    let _ = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

pub fn set_autostart(exe: Option<&Path>) {
    let Some(home) = dirs::home_dir() else { return };
    let plist = home.join("Library/LaunchAgents/com.claudepet.agent.plist");
    match exe {
        Some(exe) => {
            let body = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.claudepet.agent</string>
  <key>ProgramArguments</key><array><string>{}</string><string>run</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
</dict></plist>
"#,
                exe.display()
            );
            let _ = std::fs::create_dir_all(plist.parent().unwrap());
            let _ = std::fs::write(&plist, body);
        }
        None => {
            let _ = std::fs::remove_file(&plist);
        }
    }
}

pub fn attach_console() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_procargs() {
        let mut buf = 3i32.to_ne_bytes().to_vec();
        buf.extend(b"/bin/codex\0\0\0\0codex\0app-server\0--listen\0HOME=/x\0");
        assert_eq!(parse_procargs(&buf), ["codex", "app-server", "--listen"]);
        assert!(parse_procargs(&[1, 0]).is_empty());
    }

    #[test]
    fn finds_the_repo_root_and_scripts_by_terminal() {
        let here = env!("CARGO_MANIFEST_DIR");
        assert_eq!(workspace_of(&format!("{here}/src/platform")), here);
        assert!(select_tab_script("com.apple.Terminal", "/dev/ttys001").unwrap().contains("tty of t is \"/dev/ttys001\""));
        assert!(select_tab_script("com.googlecode.iterm2", "/dev/ttys001").unwrap().contains("sessions of t"));
        assert!(select_tab_script("com.microsoft.VSCode", "/dev/ttys001").is_none());
    }

    #[test]
    fn sees_itself() {
        let me = std::process::id();
        let exe = std::env::current_exe().unwrap();
        let name = c_text(&bsd_info(me).unwrap().pbi_comm);
        let found = processes_named(&name);
        let p = found.iter().find(|p| p.pid == me).expect("el propio proceso");
        assert!(p.started > 0);
        assert_eq!(p.cwd.as_deref().map(std::path::Path::new), Some(std::env::current_dir().unwrap().as_path()));
        assert_eq!(std::path::Path::new(&p.args[0]).file_name(), exe.file_name());
    }
}
