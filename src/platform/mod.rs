//! Todo lo que depende del sistema operativo. Cada backend expone la misma API:
//! `Surface`, `work_areas`, `cursor_pos`, `left_button_down`, `pid_alive`,
//! `find_claude_pid`, `spawn_detached`, `set_autostart`, `attach_console`,
//! `window_attributes`.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

/// Recorre la cadena de procesos padre buscando el de Claude Code.
/// `parent_of` devuelve (ppid, nombre del ejecutable).
pub fn find_ancestor(start: u32, parent_of: impl Fn(u32) -> Option<(u32, String)>) -> Option<u32> {
    let mut pid = start;
    let mut node = None;
    for _ in 0..12 {
        let (ppid, _) = parent_of(pid)?;
        if ppid == 0 || ppid == pid {
            break;
        }
        let (_, name) = parent_of(ppid).unwrap_or((0, String::new()));
        let name = name.to_lowercase();
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name);
        if base.starts_with("claude") {
            return Some(ppid);
        }
        if node.is_none() && base.starts_with("node") {
            node = Some(ppid);
        }
        pid = ppid;
    }
    node
}
