//! `claude-pet hook`: lo ejecuta Claude Code en cada evento. Debe ser rápido y
//! nunca fallar, para no frenar la sesión.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use crate::{ipc, platform};

pub fn run() {
    let mut input = String::new();
    let _ = std::io::stdin().lock().take(1 << 20).read_to_string(&mut input);
    let mut v: Value = serde_json::from_str(input.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
    if !v.is_object() {
        return;
    }
    if let Some(pid) = platform::find_claude_pid() {
        v["pet_pid"] = pid.into();
    }

    let timeout = Duration::from_millis(300);
    if ipc::send(&v, timeout) {
        return;
    }
    // La app no está corriendo: la lanzamos (salvo que la sesión esté terminando).
    if v["hook_event_name"].as_str() == Some("SessionEnd") {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        platform::spawn_detached(&exe, &["run"]);
    }
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        if ipc::send(&v, timeout) {
            return;
        }
    }
}
