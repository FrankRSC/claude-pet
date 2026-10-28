// En release no abrimos consola: la app vive en la bandeja.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod codex;
mod geom;
mod hook_client;
mod installer;
mod ipc;
mod pet;
mod platform;
mod pokedex;
mod render;
mod skins;
mod sound;
mod sprites;
mod updater;

/// Puerto local donde la app escucha los eventos de los hooks.
pub const PORT: u16 = 47823;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    match args.get(1).map(String::as_str).unwrap_or("run") {
        "run" => app::run(has("--demo"), has("--wait")),
        // Sin --agent es Claude Code (los hooks de versiones anteriores no lo mandan).
        "hook" => hook_client::run(value("--agent").as_deref().unwrap_or("claude")),
        "install" => {
            platform::attach_console();
            installer::install(!has("--no-autostart"), !has("--no-pokemon"), value("--agent").as_deref().unwrap_or("all"));
        }
        "pokemon" => {
            // Vuelve a bajar los avatares de la Pokédex (reemplaza skins.json).
            platform::attach_console();
            pokedex::install();
            ipc::send(&serde_json::json!({ "cmd": "reload" }), std::time::Duration::from_millis(500));
        }
        "uninstall" => {
            platform::attach_console();
            installer::uninstall(value("--agent").as_deref().unwrap_or("all"));
        }
        "quit" => ipc::send_quit(),
        "sound" => {
            // Prueba de sonido: el grito del Pokémon N (o el del sistema).
            platform::attach_console();
            match args.get(2).and_then(|s| s.parse::<u32>().ok()) {
                Some(id) => match sound::cry(id) {
                    Some(pcm) => {
                        let secs = sound::duration(&pcm).as_secs_f32();
                        if platform::play_wav(sound::wav(&pcm, 0.65)) {
                            println!("Sonando el grito #{id} ({secs:.1} s)");
                        } else {
                            eprintln!("El sistema no quiso tocar el grito #{id}");
                        }
                        std::thread::sleep(sound::duration(&pcm) + std::time::Duration::from_millis(300));
                    }
                    None => eprintln!("No hay grito para #{id}: genera los avatares con tools/pokedex.py y usa `claude-pet skins`."),
                },
                None => {
                    platform::play_system(platform::SystemSound::Done);
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                }
            }
        }
        "focus" => {
            // Diagnóstico de "ir a la sesión": lo mismo que el doble clic.
            platform::attach_console();
            match args.get(2).and_then(|s| s.parse::<u32>().ok()) {
                Some(pid) => {
                    let cwd = args.get(3).map_or("", String::as_str);
                    match platform::focus_session(pid, cwd) {
                        Some(app) => println!("Voy a {app}"),
                        None => eprintln!("El proceso {pid} no tiene una ventana a la cual ir."),
                    }
                    // El cambio de foco corre en otro hilo.
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
                None => eprintln!("uso: claude-pet focus <pid> [carpeta]"),
            }
        }
        "restart" => {
            // Lo mismo que "Reiniciar Claude Pet" en el menú; si no corre, la arranca.
            let msg = serde_json::json!({ "cmd": "restart" });
            if !ipc::send(&msg, std::time::Duration::from_millis(500)) {
                if let Ok(exe) = std::env::current_exe() {
                    platform::spawn_detached(&exe, &["run"]);
                }
            }
        }
        "size" => {
            platform::attach_console();
            match args.get(2).and_then(|s| s.parse::<u32>().ok()) {
                Some(px) if (pet::MIN_CELL..=pet::MAX_CELL).contains(&px) => {
                    let msg = serde_json::json!({ "cmd": "size", "px": px });
                    if !ipc::send(&msg, std::time::Duration::from_millis(500)) {
                        eprintln!("Claude Pet no está corriendo.");
                    }
                }
                _ => eprintln!(
                    "uso: claude-pet size <{}-{}>  ({} = normal)",
                    pet::MIN_CELL,
                    pet::MAX_CELL,
                    pet::DEFAULT_CELL
                ),
            }
        }
        "skins" => {
            // Instala un skins.json (el que genera tools/pokedex.py) y recarga la app.
            platform::attach_console();
            match args.get(2) {
                Some(src) => skins::install(std::path::Path::new(src)),
                None => eprintln!("uso: claude-pet skins <skins.json>  (lo genera tools/pokedex.py)"),
            }
        }
        "floors" => {
            // Diagnóstico: qué ventanas ve y dónde puede pararse un monito.
            platform::attach_console();
            let areas = platform::work_areas();
            let windows = platform::windows();
            println!("{} ventanas, áreas {:?}", windows.len(), areas);
            for (id, r) in &windows {
                println!("  ventana {id:x}: {r:?}");
            }
            for f in geom::floors(&windows, &areas, pet::size()) {
                println!("  plataforma {:x}: y={} x={}..{}", f.id, f.y, f.x0, f.x1);
            }
        }
        _ => {
            platform::attach_console();
            eprintln!(
                "uso: claude-pet [run [--demo] | hook [--agent <nombre>] | install [--no-autostart] [--no-pokemon] [--agent claude|codex|gemini|opencode|all] | uninstall [--agent …] | pokemon | skins <archivo.json> | size <n> | sound [n] | focus <pid> | restart | quit]"
            );
        }
    }
}
