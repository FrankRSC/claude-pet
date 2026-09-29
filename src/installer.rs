//! `install` / `uninstall`: copia el binario a una ruta estable y registra los
//! hooks en ~/.claude/settings.json sin tocar el resto de la configuración.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::{ipc, platform};

const EVENTS: [&str; 5] = [
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "Notification",
    "SessionEnd",
];
const MARKER: &str = "claude-pet";

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .expect("sin carpeta de usuario")
        .join(".claude")
        .join("settings.json")
}

fn install_path() -> PathBuf {
    let name = if cfg!(windows) { "claude-pet.exe" } else { "claude-pet" };
    dirs::data_local_dir()
        .expect("sin carpeta de datos")
        .join("claude-pet")
        .join(name)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn read_settings(path: &Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_settings(path: &Path, settings: &Value) -> std::io::Result<()> {
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.bak"))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)? + "\n")
}

/// Quita nuestras entradas de hooks y deja intactas las demás.
fn strip_ours(settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let is_ours = |group: &Value| {
        group["hooks"].as_array().is_some_and(|hs| {
            hs.iter()
                .any(|h| h["command"].as_str().is_some_and(|c| c.contains(MARKER)))
        })
    };
    for ev in EVENTS {
        if let Some(groups) = hooks.get_mut(ev).and_then(Value::as_array_mut) {
            groups.retain(|g| !is_ours(g));
            if groups.is_empty() {
                hooks.remove(ev);
            }
        }
    }
    if hooks.is_empty() {
        settings.as_object_mut().unwrap().remove("hooks");
    }
}

pub fn install(autostart: bool) {
    let src = std::env::current_exe().expect("no encuentro mi ejecutable");
    let dst = install_path();
    if !same_file(&src, &dst) {
        // Cerramos una versión anterior que pudiera tener el .exe bloqueado.
        ipc::send_quit();
        std::thread::sleep(Duration::from_millis(700));
        std::fs::create_dir_all(dst.parent().unwrap()).expect("no pude crear la carpeta");
        let mut copied = false;
        for _ in 0..10 {
            if std::fs::copy(&src, &dst).is_ok() {
                copied = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if !copied {
            eprintln!("No pude copiar el ejecutable a {}", dst.display());
            return;
        }
    }

    let path = settings_path();
    let mut settings = read_settings(&path);
    if !settings.is_object() {
        eprintln!("{} no es un objeto JSON; no lo toco.", path.display());
        return;
    }
    strip_ours(&mut settings);
    let command = format!("\"{}\" hook", dst.display().to_string().replace('\\', "/"));
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    for ev in EVENTS {
        let groups = hooks
            .as_object_mut()
            .unwrap()
            .entry(ev)
            .or_insert_with(|| json!([]));
        if let Some(arr) = groups.as_array_mut() {
            arr.push(json!({ "hooks": [{ "type": "command", "command": command, "timeout": 10 }] }));
        }
    }
    if let Err(e) = write_settings(&path, &settings) {
        eprintln!("No pude escribir {}: {e}", path.display());
        return;
    }

    if autostart {
        platform::set_autostart(Some(&dst));
    }
    platform::spawn_detached(&dst, &["run"]);

    println!("Claude Pet instalado en {}", dst.display());
    println!("Hooks agregados a {} (respaldo en settings.json.bak)", path.display());
    if autostart {
        println!("Arranca solo al iniciar sesión.");
    }
    println!("Abre una sesión nueva de Claude Code y aparecerá tu monito.");
}

pub fn uninstall() {
    ipc::send_quit();
    let path = settings_path();
    let mut settings = read_settings(&path);
    if settings.is_object() && path.exists() {
        strip_ours(&mut settings);
        match write_settings(&path, &settings) {
            Ok(()) => println!("Hooks quitados de {}", path.display()),
            Err(e) => eprintln!("No pude escribir {}: {e}", path.display()),
        }
    }
    platform::set_autostart(None);
    println!("Claude Pet desinstalado. Puedes borrar {}", install_path().parent().unwrap().display());
}
