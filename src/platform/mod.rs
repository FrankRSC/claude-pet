//! Todo lo que depende del sistema operativo. Cada backend expone la misma API:
//! `Surface`, `work_areas`, `cursor_pos`, `left_button_down`, `pid_alive`,
//! `find_agent_pid`, `spawn_detached`, `set_autostart`, `attach_console`,
//! `window_attributes`.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

/// Recorre la cadena de procesos padre buscando el del agente. `names` va por
/// prioridad: con el primero que coincida nos quedamos de inmediato; los
/// demás (por ejemplo `node`, el proceso real de los CLI en JavaScript) solo
/// sirven si no aparece uno mejor. `parent_of` devuelve (ppid, nombre).
pub fn find_ancestor(start: u32, parent_of: impl Fn(u32) -> Option<(u32, String)>, names: &[&str]) -> Option<u32> {
    let mut pid = start;
    let mut best: Option<(usize, u32)> = None;
    for _ in 0..12 {
        let (ppid, _) = parent_of(pid)?;
        if ppid == 0 || ppid == pid {
            break;
        }
        let (_, name) = parent_of(ppid).unwrap_or((0, String::new()));
        let name = name.to_lowercase();
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name);
        if let Some(rank) = names.iter().position(|n| base.starts_with(n)) {
            if rank == 0 {
                return Some(ppid);
            }
            if best.map_or(true, |(r, _)| rank < r) {
                best = Some((rank, ppid));
            }
        }
        pid = ppid;
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::find_ancestor;

    /// hook(1) ← sh(2) ← node(3) ← codex(4) ← zsh(5) ← launchd(0).
    /// Devuelve (ppid, nombre) del proceso `pid`, como `parent_of` de cada backend.
    fn tree(pid: u32) -> Option<(u32, String)> {
        match pid {
            1 => Some((2, "claude-pet".into())),
            2 => Some((3, "sh".into())),
            3 => Some((4, "/usr/local/bin/node".into())),
            4 => Some((5, "codex".into())),
            5 => Some((0, "zsh".into())),
            _ => None,
        }
    }

    #[test]
    fn prefers_the_first_name_and_falls_back_in_order() {
        assert_eq!(find_ancestor(1, tree, &["codex", "node"]), Some(4));
        assert_eq!(find_ancestor(1, tree, &["gemini", "node"]), Some(3));
        assert_eq!(find_ancestor(1, tree, &["claude"]), None);
        // La raíz (ppid 0) corta la búsqueda sin fallar.
        assert_eq!(find_ancestor(5, tree, &["zsh"]), None);
    }
}
