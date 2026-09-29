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

/// (ppid, nombre) de un proceso, sin lanzar `ps`.
fn parent_of(pid: u32) -> Option<(u32, String)> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    if got != size {
        return None;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(info.pbi_comm.as_ptr()) }.to_string_lossy().into_owned();
    Some((info.pbi_ppid, name))
}

pub fn find_claude_pid() -> Option<u32> {
    super::find_ancestor(std::process::id(), parent_of)
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
