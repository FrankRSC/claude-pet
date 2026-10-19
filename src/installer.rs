//! `install` / `uninstall`: copia el binario a una ruta estable y registra los
//! hooks en cada agente (Claude Code, Codex, Gemini CLI, OpenCode) sin tocar
//! el resto de su configuración.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::{ipc, platform};

/// Va en el comando de cada hook: así reconocemos nuestras entradas.
const MARKER: &str = "claude-pet";
const OPENCODE_PLUGIN: &str = include_str!("../assets/opencode/claude-pet.ts");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

impl Agent {
    pub const ALL: [Agent; 4] = [Agent::Claude, Agent::Codex, Agent::Gemini, Agent::OpenCode];

    pub fn from_name(name: &str) -> Option<Agent> {
        match name.to_lowercase().as_str() {
            "claude" | "claude-code" => Some(Agent::Claude),
            "codex" => Some(Agent::Codex),
            "gemini" => Some(Agent::Gemini),
            "opencode" => Some(Agent::OpenCode),
            _ => None,
        }
    }

    /// Lo que va en `--agent` y en el campo `agent` del protocolo.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Gemini => "gemini",
            Agent::OpenCode => "opencode",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Gemini => "Gemini CLI",
            Agent::OpenCode => "OpenCode",
        }
    }

    /// Nombres del proceso del agente, por prioridad. `node` y `bun` son el
    /// proceso real de los CLI en JavaScript cuando el ejecutable es un script.
    pub fn process_names(self) -> &'static [&'static str] {
        match self {
            Agent::Claude => &["claude", "node"],
            Agent::Codex => &["codex", "node"],
            Agent::Gemini => &["gemini", "node"],
            Agent::OpenCode => &["opencode", "bun", "node"],
        }
    }

    /// Carpeta de configuración del agente; si no existe, no está instalado.
    fn dir(self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(match self {
            Agent::Claude => home.join(".claude"),
            Agent::Codex => home.join(".codex"),
            Agent::Gemini => home.join(".gemini"),
            Agent::OpenCode => home.join(".config").join("opencode"),
        })
    }

    pub fn installed(self) -> bool {
        self.dir().is_some_and(|d| d.is_dir())
    }

    /// El archivo que tocamos.
    fn config_path(self) -> Option<PathBuf> {
        let dir = self.dir()?;
        Some(match self {
            Agent::Claude | Agent::Gemini => dir.join("settings.json"),
            Agent::Codex => dir.join("hooks.json"),
            Agent::OpenCode => dir.join("plugins").join("claude-pet.ts"),
        })
    }

    /// Eventos que nos interesan, con el timeout en la unidad de cada agente
    /// (Claude y Codex: segundos; Gemini: milisegundos). OpenCode va por plugin.
    fn events(self) -> &'static [(&'static str, u64)] {
        match self {
            Agent::Claude => &[
                ("SessionStart", 10),
                ("UserPromptSubmit", 10),
                ("Stop", 10),
                ("Notification", 10),
                ("SessionEnd", 10),
            ],
            // Codex admite 3 s como máximo en SessionEnd.
            Agent::Codex => &[
                ("SessionStart", 10),
                ("UserPromptSubmit", 10),
                ("Stop", 10),
                ("PermissionRequest", 10),
                ("SessionEnd", 3),
            ],
            Agent::Gemini => &[
                ("SessionStart", 10_000),
                ("BeforeAgent", 10_000),
                ("AfterAgent", 10_000),
                ("Notification", 10_000),
                ("SessionEnd", 10_000),
            ],
            Agent::OpenCode => &[],
        }
    }

    /// Mete nuestros hooks en `settings`, quitando antes los que hubiera.
    fn add_hooks(self, settings: &mut Value, command: &str) {
        self.strip_hooks(settings);
        let hooks = settings.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        for (ev, timeout) in self.events() {
            let groups = hooks.as_object_mut().unwrap().entry(*ev).or_insert_with(|| json!([]));
            if !groups.is_array() {
                *groups = json!([]);
            }
            let mut hook = json!({ "type": "command", "command": command, "timeout": timeout });
            let mut group = json!({});
            if self == Agent::Gemini {
                hook["name"] = MARKER.into();
                group["matcher"] = "*".into();
            }
            group["hooks"] = json!([hook]);
            groups.as_array_mut().unwrap().push(group);
        }
    }

    /// Quita nuestras entradas de hooks (de cualquier evento) y deja intactas las demás.
    fn strip_hooks(self, settings: &mut Value) {
        let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
            return;
        };
        let is_ours = |group: &Value| {
            group["hooks"]
                .as_array()
                .is_some_and(|hs| hs.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(MARKER))))
        };
        for groups in hooks.values_mut() {
            if let Some(arr) = groups.as_array_mut() {
                arr.retain(|g| !is_ours(g));
            }
        }
        hooks.retain(|_, groups| groups.as_array().is_none_or(|a| !a.is_empty()));
        if hooks.is_empty() {
            settings.as_object_mut().unwrap().remove("hooks");
        }
    }

    fn install(self, bin: &Path) -> Result<String, String> {
        let path = self.config_path().ok_or("sin carpeta de usuario")?;
        let bin = bin.display().to_string().replace('\\', "/");
        if self == Agent::OpenCode {
            let body = OPENCODE_PLUGIN.replace("__CLAUDE_PET_BIN__", &bin);
            write_file(&path, &body)?;
            return Ok(format!("plugin en {}", path.display()));
        }
        let mut settings = read_settings(&path)?;
        self.add_hooks(&mut settings, &format!("\"{bin}\" hook --agent {}", self.name()));
        write_settings(&path, &settings)?;
        Ok(format!("hooks en {} (respaldo en .bak)", path.display()))
    }

    /// Ok(None) si no había nada nuestro que quitar.
    fn uninstall(self) -> Result<Option<String>, String> {
        let path = self.config_path().ok_or("sin carpeta de usuario")?;
        if !path.exists() {
            return Ok(None);
        }
        if self == Agent::OpenCode {
            std::fs::remove_file(&path).map_err(|e| format!("no pude borrar {}: {e}", path.display()))?;
            return Ok(Some(format!("plugin quitado de {}", path.display())));
        }
        let mut settings = read_settings(&path)?;
        self.strip_hooks(&mut settings);
        if self == Agent::Codex && settings.as_object().is_some_and(|o| o.is_empty()) {
            // hooks.json era solo nuestro.
            std::fs::remove_file(&path).map_err(|e| format!("no pude borrar {}: {e}", path.display()))?;
        } else {
            write_settings(&path, &settings)?;
        }
        Ok(Some(format!("hooks quitados de {}", path.display())))
    }
}

/// `--agent all` son los agentes cuya carpeta existe; un nombre, ese aunque no exista.
fn select(agent: &str) -> Result<Vec<Agent>, String> {
    if agent == "all" {
        return Ok(Agent::ALL.into_iter().filter(|a| a.installed()).collect());
    }
    Agent::from_name(agent)
        .map(|a| vec![a])
        .ok_or_else(|| format!("Agente desconocido: {agent}. Usa claude, codex, gemini, opencode o all."))
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

/// `{}` si el archivo no existe. Si existe y no es un objeto JSON (Gemini
/// admite comentarios, por ejemplo), no lo tocamos: mejor fallar que borrarlo.
fn read_settings(path: &Path) -> Result<Value, String> {
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(json!({})) };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => Ok(v),
        _ => Err(format!("{} no es un objeto JSON (¿comentarios?); no lo toco", path.display())),
    }
}

fn write_settings(path: &Path, settings: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())? + "\n";
    if path.exists() {
        let bak = path.with_extension("json.bak");
        std::fs::copy(path, &bak).map_err(|e| format!("no pude respaldar en {}: {e}", bak.display()))?;
    }
    write_file(path, &text)
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("no pude crear {}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("no pude escribir {}: {e}", path.display()))
}

pub fn install(autostart: bool, agent: &str) {
    let agents = match select(agent) {
        Ok(a) => a,
        Err(e) => return eprintln!("{e}"),
    };
    if agents.is_empty() {
        eprintln!("No encontré ningún agente (~/.claude, ~/.codex, ~/.gemini ni ~/.config/opencode).");
        eprintln!("Puedes forzar uno con --agent <claude|codex|gemini|opencode>.");
        return;
    }

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

    let mut done = Vec::new();
    for a in &agents {
        match a.install(&dst) {
            Ok(msg) => {
                println!("{}: {msg}", a.display());
                done.push(*a);
            }
            Err(e) => eprintln!("{}: {e}", a.display()),
        }
    }
    if done.is_empty() {
        eprintln!("No quedó ningún hook instalado.");
        return;
    }

    if autostart {
        platform::set_autostart(Some(&dst));
    }
    platform::spawn_detached(&dst, &["run"]);

    println!("Claude Pet instalado en {}", dst.display());
    if autostart {
        println!("Arranca solo al iniciar sesión.");
    }
    if done.contains(&Agent::Codex) {
        println!("Codex no corre hooks sin aprobarlos: abre Codex y ejecuta /hooks para aprobar el de claude-pet.");
    }
    println!("Abre una sesión nueva y aparecerá tu monito.");
}

pub fn uninstall(agent: &str) {
    let agents = match agent {
        "all" => Agent::ALL.to_vec(),
        _ => match select(agent) {
            Ok(a) => a,
            Err(e) => return eprintln!("{e}"),
        },
    };
    ipc::send_quit();
    for a in agents {
        match a.uninstall() {
            Ok(Some(msg)) => println!("{}: {msg}", a.display()),
            Ok(None) => {}
            Err(e) => eprintln!("{}: {e}", a.display()),
        }
    }
    platform::set_autostart(None);
    println!("Claude Pet desinstalado. Puedes borrar {}", install_path().parent().unwrap().display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_are_added_and_stripped_without_touching_the_rest() {
        let mut s = json!({
            "model": "x",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "echo mio" } ] } ] }
        });
        Agent::Claude.add_hooks(&mut s, "\"/x/claude-pet\" hook --agent claude");
        assert_eq!(s["model"], "x");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2, "el hook ajeno sigue ahí");
        assert_eq!(s["hooks"]["SessionStart"][0]["hooks"][0]["timeout"], 10);
        assert!(s["hooks"]["SessionStart"][0].get("matcher").is_none());

        // Instalar dos veces no duplica.
        Agent::Claude.add_hooks(&mut s, "\"/x/claude-pet\" hook --agent claude");
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2);

        Agent::Claude.strip_hooks(&mut s);
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(s["hooks"].get("SessionStart").is_none());

        // Sin nada nuestro ni ajeno, "hooks" desaparece.
        let mut only_ours = json!({});
        Agent::Codex.add_hooks(&mut only_ours, "\"/x/claude-pet\" hook --agent codex");
        assert_eq!(only_ours["hooks"]["PermissionRequest"][0]["hooks"][0]["command"], "\"/x/claude-pet\" hook --agent codex");
        assert_eq!(only_ours["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 3);
        Agent::Codex.strip_hooks(&mut only_ours);
        assert_eq!(only_ours, json!({}));
    }

    #[test]
    fn gemini_hooks_have_name_matcher_and_milliseconds() {
        let mut s = json!({});
        Agent::Gemini.add_hooks(&mut s, "\"/x/claude-pet\" hook --agent gemini");
        let group = &s["hooks"]["AfterAgent"][0];
        assert_eq!(group["matcher"], "*");
        assert_eq!(group["hooks"][0]["name"], "claude-pet");
        assert_eq!(group["hooks"][0]["timeout"], 10_000);
        assert!(s["hooks"].get("Stop").is_none());
    }

    #[test]
    fn opencode_plugin_gets_the_binary_path() {
        let body = OPENCODE_PLUGIN.replace("__CLAUDE_PET_BIN__", "C:/Users/yo/claude-pet.exe");
        assert!(body.contains("const BIN = \"C:/Users/yo/claude-pet.exe\""));
        assert!(!body.contains("__CLAUDE_PET_BIN__"));
    }

    #[test]
    fn agent_names_round_trip() {
        for a in Agent::ALL {
            assert_eq!(Agent::from_name(a.name()), Some(a));
            assert_eq!(a.process_names()[0], a.name());
        }
        assert_eq!(Agent::from_name("Claude"), Some(Agent::Claude));
        assert_eq!(Agent::from_name("cursor"), None);
    }
}
