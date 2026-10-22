//! Sesiones de Codex abiertas. Codex solo dispara hooks cuando empieza un
//! turno, así que una sesión abierta o reanudada a la que no le has escrito no
//! tendría monito. Aquí las encontramos sin hooks:
//!
//! - Cada proceso de Codex con alguien del otro lado (la TUI o `codex exec`,
//!   no el daemon `codex app-server`) tiene una carpeta de trabajo.
//! - Su sesión es el rollout más reciente de `~/.codex/sessions` con esa misma
//!   carpeta, escrito después de que arrancó el proceso (al reanudar, Codex
//!   escribe en el rollout del hilo). El id del hilo es el `session_id` de los
//!   hooks, así que cuando lleguen caen en el mismo monito.
//!
//! Los hooks de la TUI los ejecuta el daemon, que no muere al cerrar la
//! sesión; por eso el monito se ata al PID de la TUI y no al del hook.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::platform::{self, ProcInfo};

/// Subcomandos de Codex que no son una sesión de alguien.
const SERVICES: [&str; 5] = ["app-server", "mcp-server", "exec-server", "remote-control", "app"];

#[derive(Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub pid: u32,
}

/// Procesos de Codex con alguien del otro lado.
pub fn clients() -> Vec<ProcInfo> {
    platform::processes_named("codex")
        .into_iter()
        .filter(|p| !p.args.iter().skip(1).any(|a| SERVICES.contains(&a.as_str())))
        .collect()
}

/// El cliente más reciente que trabaja en `cwd`.
pub fn client_in(clients: &[ProcInfo], cwd: &str) -> Option<u32> {
    clients
        .iter()
        .filter(|c| c.cwd.as_deref().is_some_and(|d| same_dir(d, cwd)))
        .max_by_key(|c| c.started)
        .map(|c| c.pid)
}

fn sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("sessions"))
}

/// (ruta, última modificación) de cada rollout, recorriendo AAAA/MM/DD.
fn rollouts(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
            {
                if let Ok(m) = meta.modified() {
                    out.push((path, m));
                }
            }
        }
    }
    out
}

/// Lo que dice la primera línea de un rollout (`session_meta`).
#[derive(Debug, PartialEq)]
struct Meta {
    id: String,
    cwd: String,
    /// Solo la cabecera: Codex crea un hilo así al abrir, aunque luego
    /// reanudes otro.
    empty: bool,
}

/// None si no es un hilo de una persona (los subagentes tienen
/// `thread_source` distinto de "user").
fn meta(path: &Path) -> Option<Meta> {
    let file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut line = Vec::new();
    BufReader::new(file.take(4 << 20)).read_until(b'\n', &mut line).ok()?;
    let v: Value = serde_json::from_slice(&line).ok()?;
    let p = &v["payload"];
    let by_person = p["thread_source"].as_str().is_none_or(|s| s == "user") && !p["source"].is_object();
    if !by_person {
        return None;
    }
    Some(Meta {
        id: p["id"].as_str()?.to_string(),
        cwd: p["cwd"].as_str()?.to_string(),
        empty: len <= line.len() as u64,
    })
}

/// Igual, o la misma carpeta por otro camino (/tmp y /private/tmp en macOS).
fn same_dir(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches(['/', '\\']);
    let b = b.trim_end_matches(['/', '\\']);
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Un hilo visto en disco: id, carpeta, cuándo se escribió por última vez y
/// si solo tiene la cabecera.
struct Thread {
    id: String,
    cwd: String,
    modified: u64,
    empty: bool,
}

/// A cada cliente (del más nuevo al más viejo) le toca el hilo más reciente de
/// su carpeta escrito desde que arrancó (con 2 s de margen), sin repetir. Un
/// hilo con conversación gana al vacío que Codex crea al abrir: si reanudaste
/// otro, ese es el tuyo.
fn assign(clients: &[ProcInfo], mut threads: Vec<Thread>) -> Vec<Session> {
    threads.sort_by(|a, b| (a.empty, b.modified).cmp(&(b.empty, a.modified)));
    let mut clients: Vec<&ProcInfo> = clients.iter().filter(|c| c.cwd.is_some()).collect();
    clients.sort_by(|a, b| b.started.cmp(&a.started));
    let mut taken = HashSet::new();
    let mut out = Vec::new();
    for c in clients {
        let cwd = c.cwd.as_deref().unwrap();
        let since = c.started.saturating_sub(2);
        let found = threads
            .iter()
            .find(|t| t.modified >= since && !taken.contains(&t.id) && same_dir(&t.cwd, cwd));
        if let Some(t) = found {
            taken.insert(t.id.clone());
            out.push(Session { id: t.id.clone(), cwd: t.cwd.clone(), pid: c.pid });
        }
    }
    out
}

/// Las sesiones de los `clients` dados (los que todavía no tienen monito).
pub fn open_sessions(clients: &[ProcInfo]) -> Vec<Session> {
    let clients: Vec<ProcInfo> = clients.iter().filter(|c| c.cwd.is_some()).cloned().collect();
    let Some(oldest) = clients.iter().map(|c| c.started).min() else { return Vec::new() };
    let Some(dir) = sessions_dir() else { return Vec::new() };
    let since = UNIX_EPOCH + Duration::from_secs(oldest.saturating_sub(2));
    // Un hilo puede tener varios archivos: vale el más reciente.
    let mut threads: HashMap<String, Thread> = HashMap::new();
    for (path, modified) in rollouts(&dir) {
        if modified < since {
            continue;
        }
        let Some(Meta { id, cwd, empty }) = meta(&path) else { continue };
        let modified = modified.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let t = threads.entry(id.clone()).or_insert(Thread { id, cwd, modified, empty });
        t.modified = t.modified.max(modified);
        t.empty &= empty;
    }
    assign(&clients, threads.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(pid: u32, cwd: &str, started: u64) -> ProcInfo {
        ProcInfo { pid, args: vec!["codex".into()], cwd: Some(cwd.into()), started }
    }

    fn thread(id: &str, cwd: &str, modified: u64) -> Thread {
        Thread { id: id.into(), cwd: cwd.into(), modified, empty: false }
    }

    #[test]
    fn each_client_gets_its_latest_thread_since_it_started() {
        let clients = [client(10, "/p/a", 1000), client(11, "/p/a", 2000), client(12, "/p/b", 1500), client(13, "/p/c", 1000)];
        let threads = vec![
            thread("viejo-a", "/p/a", 900),  // antes de que arrancara nadie en /p/a
            thread("a1", "/p/a", 1200),      // escrito por el cliente 10
            thread("a2", "/p/a", 2100),      // el cliente 11 (más nuevo) se queda con el más reciente
            thread("b1", "/p/b/", 1499),     // margen de 2 s y barra final
            thread("otro", "/p/z", 3000),
        ];
        let mut got = assign(&clients, threads);
        got.sort_by_key(|s| s.pid);
        let pairs: Vec<(u32, &str)> = got.iter().map(|s| (s.pid, s.id.as_str())).collect();
        // El 13 no tiene hilo: es una TUI recién abierta a la que no le han escrito.
        assert_eq!(pairs, [(10, "a1"), (11, "a2"), (12, "b1")]);
    }

    #[test]
    fn a_resumed_thread_beats_the_empty_one_codex_opens() {
        let clients = [client(10, "/p/a", 1000)];
        let fresh = Thread { empty: true, ..thread("vacío", "/p/a", 1001) };
        let got = assign(&clients, vec![fresh, thread("reanudado", "/p/a", 1003)]);
        assert_eq!(got[0].id, "reanudado");
        // Sin otro hilo, el vacío es el de esa TUI (aún sin prompt).
        let fresh = Thread { empty: true, ..thread("vacío", "/p/a", 1001) };
        assert_eq!(assign(&clients, vec![fresh])[0].id, "vacío");
    }

    #[test]
    fn services_are_not_clients_and_reads_rollout_meta() {
        let dir = std::env::temp_dir().join(format!("claude-pet-codex-{}", std::process::id()));
        let day = dir.join("2026/10/19");
        std::fs::create_dir_all(&day).unwrap();
        let file = day.join("rollout-2026-10-19T10-00-00-abc.jsonl");
        std::fs::write(&file, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"/p/a\",\"thread_source\":\"user\"}}\n{\"x\":1}\n").unwrap();
        let empty = day.join("rollout-2026-10-19T10-00-01-vacio.jsonl");
        std::fs::write(&empty, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"vacio\",\"cwd\":\"/p/a\"}}\n").unwrap();
        let sub = day.join("rollout-2026-10-19T10-00-02-sub.jsonl");
        std::fs::write(&sub, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sub\",\"cwd\":\"/p/a\",\"thread_source\":\"subagent\"}}\n{}\n").unwrap();
        std::fs::write(day.join("otro.txt"), "no").unwrap();
        assert_eq!(rollouts(&dir).len(), 3);
        assert_eq!(meta(&file), Some(Meta { id: "abc".into(), cwd: "/p/a".into(), empty: false }));
        assert_eq!(meta(&empty).map(|m| m.empty), Some(true));
        assert_eq!(meta(&sub), None, "los hilos de subagentes no son sesiones");
        std::fs::remove_dir_all(&dir).unwrap();

        let is_service = |args: &[&str]| args.iter().skip(1).any(|a| SERVICES.contains(a));
        assert!(is_service(&["/x/codex", "app-server", "--listen", "unix://"]));
        assert!(!is_service(&["/x/codex"]));
        assert!(!is_service(&["/x/codex", "exec", "hola"]));
    }
}
