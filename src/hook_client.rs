//! `claude-pet hook --agent <nombre>`: lo ejecuta el agente (Claude Code,
//! Codex, Gemini CLI o el plugin de OpenCode) en cada evento, con el JSON del
//! evento por stdin. Debe ser rápido y nunca fallar, para no frenar la sesión.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use crate::installer::Agent;
use crate::{ipc, platform};

pub fn run(agent: &str) {
    let mut input = String::new();
    let _ = std::io::stdin().lock().take(1 << 20).read_to_string(&mut input);
    let mut v: Value = serde_json::from_str(input.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
    if !v.is_object() {
        return;
    }
    normalize(&mut v, agent);
    // Claude Code les pasa a sus hooks su PID y desde dónde se abrió (la
    // terminal o la pestaña Code de Claude Desktop, que usa su propia copia
    // de Claude Code y cuyo proceso no siempre se llama "claude").
    if agent == "claude" {
        if let Some(pid) = std::env::var("CLAUDE_PID").ok().and_then(|p| p.parse::<u32>().ok()).filter(|&p| platform::pid_alive(p)) {
            v["pet_pid"] = pid.into();
        }
        if std::env::var("CLAUDE_CODE_ENTRYPOINT").is_ok_and(|e| is_desktop(&e)) {
            v["desktop"] = true.into();
        }
    }
    // El plugin de OpenCode ya manda su PID; a los demás se los buscamos.
    if v["pet_pid"].as_u64().is_none() {
        let names: Vec<&str> = match Agent::from_name(agent) {
            Some(a) => a.process_names().to_vec(),
            None => vec![agent, "node"],
        };
        if let Some(pid) = platform::find_agent_pid(&names) {
            v["pet_pid"] = pid.into();
        }
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

/// La sesión es de Claude Desktop (su `entrypoint`; en la terminal es "cli").
pub fn is_desktop(entrypoint: &str) -> bool {
    entrypoint.to_lowercase().contains("desktop")
}

/// Traduce el evento del agente a uno de los cinco que entiende la app
/// (`SessionStart`, `UserPromptSubmit`, `Stop`, `Notification`, `SessionEnd`)
/// y anota de qué agente viene. Lo que no se reconoce pasa tal cual.
pub fn normalize(v: &mut Value, agent: &str) {
    let event = v["hook_event_name"].as_str().unwrap_or("").to_string();
    let mapped = match (agent, event.as_str()) {
        ("codex", "PermissionRequest") => "Notification",
        ("gemini", "BeforeAgent") => "UserPromptSubmit",
        ("gemini", "AfterAgent") => "Stop",
        _ => event.as_str(),
    };
    if mapped != event {
        v["hook_event_name"] = mapped.into();
    }
    v["agent"] = agent.into();
    // Codex no manda texto con el permiso; lo armamos con la herramienta.
    if mapped == "Notification" && v["message"].as_str().is_none() {
        if let Some(tool) = v["tool_name"].as_str() {
            v["message"] = format!("Permiso para {tool}").into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_desktop, normalize};
    use serde_json::json;

    fn mapped(agent: &str, event: &str) -> String {
        let mut v = json!({ "hook_event_name": event, "session_id": "s" });
        normalize(&mut v, agent);
        assert_eq!(v["agent"], agent);
        v["hook_event_name"].as_str().unwrap().to_string()
    }

    #[test]
    fn maps_each_agent_to_the_five_internal_events() {
        for ev in ["SessionStart", "UserPromptSubmit", "Stop", "Notification", "SessionEnd"] {
            assert_eq!(mapped("claude", ev), ev);
            assert_eq!(mapped("opencode", ev), ev);
        }
        assert_eq!(mapped("codex", "PermissionRequest"), "Notification");
        assert_eq!(mapped("codex", "Stop"), "Stop");
        assert_eq!(mapped("gemini", "BeforeAgent"), "UserPromptSubmit");
        assert_eq!(mapped("gemini", "AfterAgent"), "Stop");
        assert_eq!(mapped("gemini", "Notification"), "Notification");
        // Lo desconocido pasa tal cual (la app lo ignora).
        assert_eq!(mapped("codex", "PreToolUse"), "PreToolUse");
        assert_eq!(mapped("cursor", "Stop"), "Stop");
    }

    #[test]
    fn desktop_sessions_are_recognized() {
        assert!(is_desktop("claude-desktop"));
        assert!(is_desktop("Claude Desktop"));
        assert!(!is_desktop("cli"));
        assert!(!is_desktop("sdk-ts"));
    }

    #[test]
    fn codex_permission_gets_a_message() {
        let mut v = json!({ "hook_event_name": "PermissionRequest", "tool_name": "Bash" });
        normalize(&mut v, "codex");
        assert_eq!(v["message"], "Permiso para Bash");
        // Si ya viene texto, se respeta.
        let mut v = json!({ "hook_event_name": "Notification", "message": "hola", "tool_name": "x" });
        normalize(&mut v, "gemini");
        assert_eq!(v["message"], "hola");
    }
}
