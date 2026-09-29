// En release no abrimos consola: la app vive en la bandeja.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod geom;
mod hook_client;
mod installer;
mod ipc;
mod pet;
mod platform;
mod render;
mod skins;
mod sprites;

/// Puerto local donde la app escucha los eventos de los hooks.
pub const PORT: u16 = 47823;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match args.get(1).map(String::as_str).unwrap_or("run") {
        "run" => app::run(has("--demo")),
        "hook" => hook_client::run(),
        "install" => {
            platform::attach_console();
            installer::install(!has("--no-autostart"));
        }
        "uninstall" => {
            platform::attach_console();
            installer::uninstall();
        }
        "quit" => ipc::send_quit(),
        "size" => {
            platform::attach_console();
            match args.get(2).and_then(|s| s.parse::<u32>().ok()) {
                Some(px) if (pet::MIN_CELL..=pet::MAX_CELL).contains(&px) => {
                    let msg = serde_json::json!({ "cmd": "size", "px": px });
                    if !ipc::send(&msg, std::time::Duration::from_millis(500)) {
                        eprintln!("Claude Pet no está corriendo.");
                    }
                }
                _ => eprintln!("uso: claude-pet size <{}-{}>  (5 = normal)", pet::MIN_CELL, pet::MAX_CELL),
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
                "uso: claude-pet [run [--demo] | hook | install [--no-autostart] | uninstall | quit]"
            );
        }
    }
}
