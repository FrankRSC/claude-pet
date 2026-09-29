//! App residente: event loop de winit, una ventana por monito, bandeja y avisos.

use std::time::{Duration, Instant};

use serde_json::Value;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId, WindowLevel};

use crate::geom::{self, area_at, Rect};
use crate::pet::{self, fmt_duration, hash, size, Mode, Pet, World};
use crate::platform::{self, Surface};
use crate::render::{off, Renderer, BUF};
use crate::sprites::Look;
use crate::{ipc, sprites, updater};

/// Cuadros por segundo según lo que hagan los monitos: solo lo que vuela o
/// arrastras necesita 60; el sprite se anima a 4 por segundo.
const FAST: Duration = Duration::from_millis(16);
const MOVING: Duration = Duration::from_millis(50);
const STILL: Duration = Duration::from_millis(125);
const ASLEEP: Duration = Duration::from_millis(500);
/// Sin monitos: solo hay que despertar para los temporizadores largos. Los
/// hooks, el menú y la carpeta de sesiones despiertan al loop por su cuenta.
const EMPTY: Duration = Duration::from_secs(60);
/// La lista de ventanas del sistema es lo más caro de cada cuadro.
const FLOORS_EVERY: Duration = Duration::from_millis(200);
/// Pasos de física máximos, para que un cuadro lento no atraviese el piso.
const MAX_STEP: f32 = 0.05;

pub enum UserEvent {
    Hook(Value),
    Menu(MenuEvent),
    /// Algo cambió en ~/.claude/sessions: puede haber una sesión nueva.
    SessionsChanged,
    /// La revisión automática encontró esta versión publicada.
    Available(String),
    /// Terminó de buscar (e instalar) una actualización.
    Update(Result<updater::Outcome, String>),
}

struct Press {
    start: (f32, f32),
    grab: (f32, f32),
    at: Instant,
    dragging: bool,
    last: (f32, f32),
    last_at: Instant,
    vel: (f32, f32),
}

struct PetWindow {
    pet: Pet,
    avatar: usize,
    look: Look,
    buf: Vec<u32>,
    surface: Surface,
    window: Window,
    press: Option<Press>,
    shown: bool,
    /// Cómo se veía el último cuadro dibujado (`Renderer::visual_key` + avatar)
    /// y dónde: si no cambian, no se dibuja ni se toca la ventana.
    last_frame: u64,
    last_pos: (i32, i32),
}

struct Tray {
    _icon: TrayIcon,
    demo: MenuId,
    hide: CheckMenuItem,
    notify: CheckMenuItem,
    /// [0] = "Aleatorio"; [i + 1] = avatar i.
    avatars: Vec<CheckMenuItem>,
    /// Mismo orden que `SIZES`.
    sizes: Vec<CheckMenuItem>,
    update: MenuItem,
    quit: MenuId,
}

struct App {
    pets: Vec<PetWindow>,
    renderer: Renderer,
    areas: Vec<Rect>,
    areas_at: Instant,
    floors: Vec<geom::Floor>,
    floors_at: Instant,
    low_power: bool,
    last_tick: Instant,
    last_pid_check: Instant,
    last_update_check: Instant,
    /// Versión nueva publicada, si la revisión automática encontró una.
    available: Option<String>,
    proxy: EventLoopProxy<UserEvent>,
    /// Mientras viva, el sistema nos avisa de cada cambio en ~/.claude/sessions.
    /// Si no se pudo crear, revisamos la carpeta cada pocos segundos.
    watcher: Option<notify::RecommendedWatcher>,
    last_scan: Instant,
    /// Sesiones que ya tuvieron monito; el escaneo no las revive.
    scanned: std::collections::HashSet<String>,
    tray: Option<Tray>,
    hidden: bool,
    demo_on_start: bool,
    demo_count: u32,
    /// None = cada sesión recibe uno distinto.
    avatar_choice: Option<usize>,
}

fn config_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_local_dir()?.join("claude-pet").join("config.json"))
}

/// Tamaños del menú: (nombre, píxeles por celda).
const SIZES: [(&str, u32); 6] = [
    ("Chico", 3),
    ("Mediano", 4),
    ("Normal", 5),
    ("Grande", 6),
    ("Muy grande", 7),
    ("Enorme", 8),
];

fn load_config() -> Value {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

fn save_config(key: &str, value: Value) {
    let Some(path) = config_path() else { return };
    let mut config = load_config();
    config[key] = value;
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(path, serde_json::to_string_pretty(&config).unwrap_or_default());
}

fn load_avatar_choice() -> Option<usize> {
    let config = load_config();
    let name = config["avatar"].as_str()?;
    (0..sprites::count()).find(|&i| sprites::name(i) == name)
}

fn save_avatar_choice(choice: Option<usize>) {
    save_config("avatar", choice.map_or("aleatorio", sprites::name).into());
}

/// Avatar elegido a mano para cada proyecto (por nombre de carpeta).
fn load_project_avatar(label: &str) -> Option<usize> {
    let config = load_config();
    let name = config["project_avatars"][label].as_str()?;
    (0..sprites::count()).find(|&i| sprites::name(i) == name)
}

fn save_project_avatar(label: &str, avatar: usize) {
    let mut map = load_config()["project_avatars"].clone();
    if !map.is_object() {
        map = serde_json::json!({});
    }
    map[label] = sprites::name(avatar).into();
    save_config("project_avatars", map);
}

fn load_cell_px() -> u32 {
    load_config()["cell_px"].as_u64().map_or(5, |n| n as u32)
}

/// `wait`: venimos de una actualización y la instancia anterior todavía está
/// soltando el puerto.
pub fn run(demo: bool, wait: bool) {
    let mut listener = ipc::bind();
    for _ in 0..if wait { 50 } else { 0 } {
        if listener.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        listener = ipc::bind();
    }
    updater::cleanup();
    let Some(listener) = listener else {
        // Ya hay una instancia; si pidieron demo, se la mandamos a ella.
        if demo {
            ipc::send(&demo_event(0), Duration::from_millis(500));
        }
        return;
    };
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build().expect("no pude crear el event loop");
    ipc::serve(listener, event_loop.create_proxy());
    // El menú despierta al loop en vez de revisarlo en cada cuadro.
    let proxy = std::sync::Mutex::new(event_loop.create_proxy());
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.lock().unwrap().send_event(UserEvent::Menu(e));
    }));
    pet::set_cell_px(load_cell_px());

    let now = Instant::now();
    let mut app = App {
        pets: Vec::new(),
        renderer: Renderer::new(),
        areas: platform::work_areas(),
        areas_at: now,
        floors: Vec::new(),
        floors_at: now,
        low_power: platform::low_power(),
        last_tick: now,
        last_pid_check: now,
        last_update_check: now,
        available: None,
        proxy: event_loop.create_proxy(),
        watcher: watch_sessions(event_loop.create_proxy()),
        last_scan: now,
        scanned: Default::default(),
        tray: None,
        hidden: false,
        demo_on_start: demo,
        demo_count: 0,
        avatar_choice: load_avatar_choice(),
    };
    let _ = event_loop.run_app(&mut app);
}

fn demo_event(n: u32) -> Value {
    serde_json::json!({
        "hook_event_name": "SessionStart",
        "session_id": format!("demo-{n}-{}", std::process::id()),
        "cwd": "demo",
    })
}

fn sessions_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("sessions"))
}

/// Claude Code mantiene un ~/.claude/sessions/<pid>.json por proceso. Vigilamos
/// esa carpeta para darle monito a cada sesión en cuanto aparece, aunque no
/// mande ningún hook (por ejemplo, si abrió antes de instalar).
fn watch_sessions(proxy: EventLoopProxy<UserEvent>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let dir = sessions_dir()?;
    let _ = std::fs::create_dir_all(&dir);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok_and(|e| !e.kind.is_access()) {
            let _ = proxy.send_event(UserEvent::SessionsChanged);
        }
    })
    .ok()?;
    watcher.watch(&dir, notify::RecursiveMode::NonRecursive).ok()?;
    Some(watcher)
}

/// (session_id, pid, cwd) de cada registro en ~/.claude/sessions, vivo o no.
fn session_records() -> Vec<(String, u32, String)> {
    let Some(Ok(entries)) = sessions_dir().map(std::fs::read_dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<Value>(&s).ok())
        .filter_map(|v| {
            let session = v["sessionId"].as_str().filter(|s| !s.is_empty())?.to_string();
            let pid = v["pid"].as_u64()? as u32;
            Some((session, pid, v["cwd"].as_str().unwrap_or("").to_string()))
        })
        .collect()
}

fn update_label(available: Option<&str>) -> String {
    match available {
        Some(v) => format!("Actualizar a v{v}"),
        None => format!("Buscar actualización (v{})", updater::current()),
    }
}

/// Revisa en segundo plano si hay versión nueva y avisa al event loop.
fn check_available(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        if let Some(v) = updater::available() {
            let _ = proxy.send_event(UserEvent::Available(v));
        }
    });
}

fn label_from_cwd(cwd: &str) -> String {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("");
    if name.is_empty() { "Claude".into() } else { name.into() }
}

fn notify(summary: String, body: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary(&summary)
            .body(&body)
            .appname("Claude Pet")
            .show();
    });
}

impl App {
    fn notifications_on(&self) -> bool {
        self.tray.as_ref().map_or(true, |t| t.notify.is_checked())
    }

    fn build_tray(&mut self) {
        let menu = Menu::new();
        let demo = MenuItem::new("Monito de prueba", true, None);
        let hide = CheckMenuItem::new("Ocultar monitos", true, false, None);
        let notify = CheckMenuItem::new("Notificaciones", true, true, None);
        let update = MenuItem::new(update_label(self.available.as_deref()), true, None);
        let quit = MenuItem::new("Salir", true, None);
        let avatar_menu = Submenu::new("Avatar", true);
        let mut avatars = vec![CheckMenuItem::new("Aleatorio (uno por sesión)", true, self.avatar_choice.is_none(), None)];
        for i in 0..sprites::count() {
            avatars.push(CheckMenuItem::new(sprites::name(i), true, self.avatar_choice == Some(i), None));
        }
        let _ = avatar_menu.append(&avatars[0]);
        let _ = avatar_menu.append(&PredefinedMenuItem::separator());
        for item in &avatars[1..] {
            let _ = avatar_menu.append(item);
        }
        let size_menu = Submenu::new("Tamaño", true);
        let sizes: Vec<CheckMenuItem> = SIZES
            .iter()
            .map(|(name, px)| CheckMenuItem::new(*name, true, pet::cell_px() == *px, None))
            .collect();
        for item in &sizes {
            let _ = size_menu.append(item);
        }
        let _ = menu.append_items(&[
            &demo,
            &avatar_menu,
            &size_menu,
            &hide,
            &notify,
            &PredefinedMenuItem::separator(),
            &update,
            &quit,
        ]);
        let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Claude Pet");
        if let Ok(icon) = Icon::from_rgba(sprites::icon_rgba(), 32, 32) {
            builder = builder.with_icon(icon);
        }
        if let Ok(icon) = builder.build() {
            self.tray = Some(Tray {
                _icon: icon,
                demo: demo.id().clone(),
                hide,
                notify,
                avatars,
                sizes,
                update,
                quit: quit.id().clone(),
            });
        }
    }

    /// Cambia el tamaño de todos los monitos manteniendo los pies en su lugar.
    fn choose_size(&mut self, px: u32) {
        let old = size();
        pet::set_cell_px(px);
        save_config("cell_px", px.into());
        let grow = size() - old;
        for pw in &mut self.pets {
            pw.pet.x -= grow / 2.0;
            pw.pet.y -= grow;
        }
        if let Some(tray) = &self.tray {
            for (item, (_, p)) in tray.sizes.iter().zip(SIZES) {
                item.set_checked(p == px);
            }
        }
    }

    /// El avatar fijo elegido, o uno que dependa de la sesión y que no repita
    /// ninguno de los monitos que ya están en pantalla (mientras alcancen).
    fn pick_avatar(&self, session: &str, label: &str) -> usize {
        if let Some(i) = load_project_avatar(label) {
            return i;
        }
        if let Some(i) = self.avatar_choice {
            return i;
        }
        let n = sprites::count();
        let start = (hash(session) % n as u64) as usize;
        (0..n)
            .map(|k| (start + k) % n)
            .find(|i| self.pets.iter().all(|pw| pw.avatar != *i))
            .unwrap_or(start)
    }

    fn set_avatar(pw: &mut PetWindow, avatar: usize) {
        pw.avatar = avatar;
        pw.look = sprites::look(avatar, pw.pet.color);
    }

    /// Clic central o ⌥+clic: el siguiente avatar, solo para este monito. Se
    /// recuerda para su proyecto.
    fn next_avatar(pw: &mut PetWindow) {
        let next = (pw.avatar + 1) % sprites::count();
        Self::set_avatar(pw, next);
        pw.pet.say(sprites::name(next), 1.5);
        save_project_avatar(&pw.pet.label, next);
    }

    fn choose_avatar(&mut self, choice: Option<usize>) {
        self.avatar_choice = choice;
        save_avatar_choice(choice);
        // Elegir desde el menú manda sobre lo que se eligió monito por monito.
        save_config("project_avatars", serde_json::json!({}));
        if let Some(tray) = &self.tray {
            for (k, item) in tray.avatars.iter().enumerate() {
                item.set_checked(if k == 0 { choice.is_none() } else { choice == Some(k - 1) });
            }
        }
        // Reasignamos a todos, uno por uno, para que en modo aleatorio no se repitan.
        let pets = std::mem::take(&mut self.pets);
        for mut pw in pets {
            let avatar = self.pick_avatar(&pw.pet.session, &pw.pet.label);
            Self::set_avatar(&mut pw, avatar);
            self.pets.push(pw);
        }
    }

    fn spawn_pet(&mut self, el: &ActiveEventLoop, session: String, label: String, pid: Option<u32>) -> Option<usize> {
        let attrs = Window::default_attributes()
            .with_title("Claude Pet")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_active(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(PhysicalSize::new(BUF, BUF));
        let window = el.create_window(platform::window_attributes(attrs)).ok()?;
        let surface = Surface::new(&window, BUF, BUF);
        // Aparece en el monitor donde está el cursor.
        let (cx, cy) = platform::cursor_pos();
        let area = area_at(&self.areas, cx, cy);
        let avatar = self.pick_avatar(&session, &label);
        let pet = Pet::new(session, label, pid, area, self.pets.len());
        let look = sprites::look(avatar, pet.color);
        self.pets.push(PetWindow {
            pet,
            avatar,
            look,
            buf: vec![0; (BUF * BUF) as usize],
            surface,
            window,
            press: None,
            shown: false,
            last_frame: 0,
            last_pos: (i32::MIN, i32::MIN),
        });
        // Que el primer cuadro llegue ya y no simule el rato que el loop durmió.
        self.floors_at = Instant::now() - FLOORS_EVERY;
        self.last_tick = Instant::now() - FAST;
        Some(self.pets.len() - 1)
    }

    fn scan_sessions(&mut self, el: &ActiveEventLoop) {
        for (session, pid, cwd) in session_records() {
            if self.scanned.contains(&session) || !platform::pid_alive(pid) {
                continue;
            }
            self.on_hook(
                el,
                serde_json::json!({
                    "hook_event_name": "SessionStart",
                    "session_id": session,
                    "cwd": cwd,
                    "pet_pid": pid,
                }),
            );
        }
    }

    fn on_hook(&mut self, el: &ActiveEventLoop, v: Value) {
        match v["cmd"].as_str() {
            Some("quit") => return el.exit(),
            Some("size") => {
                if let Some(px) = v["px"].as_u64() {
                    self.choose_size((px as u32).clamp(pet::MIN_CELL, pet::MAX_CELL));
                }
                return;
            }
            _ => {}
        }
        let event = v["hook_event_name"].as_str().unwrap_or("");
        let Some(session) = v["session_id"].as_str().filter(|s| !s.is_empty()) else { return };
        let pid = v["pet_pid"].as_u64().map(|p| p as u32);

        let idx = match self.pets.iter().position(|p| p.pet.session == session) {
            Some(i) => i,
            None if event == "SessionEnd" => return,
            None => {
                self.scanned.insert(session.to_string());
                debug_log(format_args!("monito nuevo: {session} ({event})"));
                let label = label_from_cwd(v["cwd"].as_str().unwrap_or(""));
                match self.spawn_pet(el, session.to_string(), label, pid) {
                    Some(i) => i,
                    None => return,
                }
            }
        };
        let notifications = self.notifications_on();
        let pet = &mut self.pets[idx].pet;
        if pid.is_some() {
            pet.pid = pid;
        }
        match event {
            "UserPromptSubmit" => pet.on_prompt(),
            "Stop" => {
                let took = pet.on_stop();
                if notifications {
                    let body = match took {
                        Some(d) => format!("{} terminó en {}", pet.label, fmt_duration(d)),
                        None => format!("{} terminó", pet.label),
                    };
                    notify("✅ Claude terminó".into(), body);
                }
            }
            "Notification" => {
                let msg = v["message"].as_str().unwrap_or("Claude necesita tu atención");
                if msg.to_lowercase().contains("waiting for your input") {
                    pet.on_waiting();
                } else {
                    pet.on_attention(msg);
                    if notifications {
                        notify(format!("🔔 {}", pet.label), msg.to_string());
                    }
                }
            }
            "SessionEnd" => pet.on_end(),
            _ => {}
        }
    }

    fn on_available(&mut self, version: String) {
        if let Some(tray) = &self.tray {
            tray.update.set_text(update_label(Some(&version)));
        }
        // Una notificación por versión, aunque la app se reinicie.
        if load_config()["notified_version"].as_str() != Some(version.as_str()) {
            notify(
                format!("Claude Pet v{version} disponible"),
                format!("Haz clic en «Actualizar a v{version}» en el menú de Claude Pet."),
            );
            save_config("notified_version", version.as_str().into());
        }
        self.available = Some(version);
    }

    fn on_update(&mut self, el: &ActiveEventLoop, result: Result<updater::Outcome, String>) {
        if let Some(tray) = &self.tray {
            tray.update.set_enabled(true);
            tray.update.set_text(update_label(self.available.as_deref()));
        }
        match result {
            Ok(updater::Outcome::UpToDate) => {
                notify("Claude Pet".into(), format!("Ya tienes la última versión ({}).", updater::current()));
            }
            Ok(updater::Outcome::Installed { version, exe }) => {
                notify("Claude Pet actualizado".into(), format!("Versión {version}. Reiniciando…"));
                platform::spawn_detached(&exe, &["run", "--wait"]);
                el.exit();
            }
            Err(e) => notify("No se pudo actualizar".into(), e),
        }
    }

    fn on_menu(&mut self, el: &ActiveEventLoop, ev: MenuEvent) {
        {
            let Some(tray) = &self.tray else { return };
            if let Some(k) = tray.avatars.iter().position(|item| *item.id() == ev.id) {
                self.choose_avatar(k.checked_sub(1));
            } else if let Some(k) = tray.sizes.iter().position(|item| *item.id() == ev.id) {
                self.choose_size(SIZES[k].1);
            } else if ev.id == tray.quit {
                el.exit();
            } else if ev.id == *tray.update.id() {
                tray.update.set_enabled(false);
                tray.update.set_text("Buscando actualización…");
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::Update(updater::check_and_install()));
                });
            } else if ev.id == tray.demo {
                self.demo_count += 1;
                let v = demo_event(self.demo_count);
                self.on_hook(el, v);
            } else if ev.id == *tray.hide.id() {
                self.hidden = tray.hide.is_checked();
                for pw in &mut self.pets {
                    pw.surface.show(&pw.window, !self.hidden);
                    pw.shown = !self.hidden;
                }
            }
        }
    }

    fn any_perched(&self) -> bool {
        self.pets.iter().any(|pw| pw.pet.perched())
    }

    /// Cada cuánto hay que dibujar, según el monito más movido. None = no hay monitos.
    fn frame_interval(&self) -> Option<Duration> {
        let need = |pw: &PetWindow| {
            let p = &pw.pet;
            if pw.press.is_some() || matches!(p.mode, Mode::Dragged | Mode::Falling) {
                FAST
            } else if matches!(p.mode, Mode::Idle | Mode::Sitting) {
                STILL
            } else if p.mode == Mode::Sleeping {
                ASLEEP
            } else {
                MOVING
            }
        };
        let mut best = self.pets.iter().map(need).min()?;
        // Estás arrastrando algo y hay un monito sobre una ventana: puede ser esa
        // ventana, así que el monito tiene que seguirla con fluidez.
        if self.any_perched() && platform::left_button_down() {
            best = best.min(MOVING);
        }
        let best = if self.hidden { ASLEEP } else { best };
        // En modo de bajo consumo, a lo más 30 por segundo y lo demás a la mitad.
        Some(if self.low_power { (best * 2).max(MOVING) } else { best })
    }

    fn tick(&mut self, el: &ActiveEventLoop, now: Instant) {
        // Tras un rato dormido el loop puede despertar con mucho dt: se simula
        // en pasos cortos para no atravesar pisos.
        let mut dt = (now - self.last_tick).as_secs_f32().min(0.5);
        self.last_tick = now;

        if now - self.areas_at > Duration::from_secs(2) {
            self.areas = platform::work_areas();
            self.low_power = platform::low_power();
            self.areas_at = now;
        }

        if self.watcher.is_none() && now - self.last_scan > Duration::from_secs(3) {
            self.last_scan = now;
            self.scan_sessions(el);
        }

        if now - self.last_update_check > Duration::from_secs(6 * 60 * 60) {
            self.last_update_check = now;
            check_available(self.proxy.clone());
        }

        // Sesiones que murieron sin mandar SessionEnd.
        if now - self.last_pid_check > Duration::from_secs(10) {
            self.last_pid_check = now;
            for pw in &mut self.pets {
                if pw.pet.pid.is_some_and(|pid| !platform::pid_alive(pid)) {
                    pw.pet.on_end();
                }
            }
        }
        if self.pets.is_empty() {
            self.floors.clear();
            return;
        }

        let cursor = platform::cursor_pos();
        let down = platform::left_button_down();
        // Los bordes de arriba de las ventanas son plataformas. Pedir la lista al
        // sistema es lo más caro del cuadro: cada cuadro solo mientras arrastras
        // (con el botón apretado) y algún monito está parado en una ventana, para
        // que la siga; si no, cada FLOORS_EVERY.
        if (down && self.any_perched()) || now - self.floors_at >= FLOORS_EVERY {
            self.floors = geom::floors(&platform::windows(), &self.areas, size());
            self.floors_at = now;
        }
        let world = World { areas: &self.areas, floors: &self.floors, cursor };
        for pw in &mut self.pets {
            drag(pw, cursor, down, now);
            let (px, py) = (pw.pet.x, pw.pet.y);
            pw.pet.hover = pw.press.is_none()
                && cursor.0 >= px + 8.0
                && cursor.0 < px + size() - 8.0
                && cursor.1 >= py + 8.0
                && cursor.1 < py + size();
        }
        while dt > 0.0 {
            let step = dt.min(MAX_STEP);
            dt -= step;
            for pw in &mut self.pets {
                pw.pet.update(step, &world);
            }
        }
        for i in 0..self.pets.len() {
            for j in 0..self.pets.len() {
                if i == j {
                    continue;
                }
                let (ox, oy) = (self.pets[j].pet.x, self.pets[j].pet.y);
                self.pets[i].pet.avoid(ox, oy);
                // Un monito lanzado tumba al que se encuentre.
                let Some((vx, vy)) = self.pets[i].pet.flying_velocity() else { continue };
                let (dx, dy) = (ox - self.pets[i].pet.x, oy - self.pets[i].pet.y);
                if dx.abs() < size() * 0.7 && dy.abs() < size() * 0.7 && self.pets[j].pet.on_ground_mode() {
                    self.pets[j].pet.knock(vx * 0.7, -350.0 + vy.min(0.0) * 0.3);
                    self.pets[i].pet.damp();
                }
            }
        }
        self.pets.retain(|pw| !pw.pet.dead());

        if self.hidden {
            return;
        }
        for pw in &mut self.pets {
            let pet = &pw.pet;
            let area = area_at(&self.areas, pet.x + size() / 2.0, pet.y + size() / 2.0);
            let room_above = pet.y - area.t > 50.0;
            // Solo se dibuja y se habla con el sistema de ventanas si algo cambió.
            let pos = ((pet.x - off()).round() as i32, (pet.y - off()).round() as i32);
            if pos != pw.last_pos {
                pw.surface.move_to(&pw.window, pos.0, pos.1);
                pw.last_pos = pos;
            }
            let frame = Renderer::visual_key(pet, room_above) ^ (pw.avatar as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            if frame != pw.last_frame {
                self.renderer.draw(pet, &pw.look, &mut pw.buf, room_above);
                pw.surface.present(&pw.window, &pw.buf);
                pw.last_frame = frame;
            }
            if !pw.shown {
                pw.surface.show(&pw.window, true);
                pw.shown = true;
            }
        }
    }
}

/// Con CLAUDE_PET_DEBUG=1 escribe en %TEMP%\claude-pet.log.
fn debug_log(msg: std::fmt::Arguments) {
    if std::env::var_os("CLAUDE_PET_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("claude-pet.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

/// Clic corto = globo con info; mantener o mover = arrastrar; soltar = lanzar.
fn drag(pw: &mut PetWindow, cursor: (f32, f32), down: bool, now: Instant) {
    let Some(p) = &mut pw.press else { return };
    debug_log(format_args!(
        "drag down={down} cursor={cursor:?} dragging={} pet=({},{}) mode={:?}",
        p.dragging, pw.pet.x, pw.pet.y, pw.pet.mode
    ));
    if down {
        let moved = ((cursor.0 - p.start.0).powi(2) + (cursor.1 - p.start.1).powi(2)).sqrt();
        if !p.dragging && (moved > 5.0 || now - p.at > Duration::from_millis(300)) {
            p.dragging = true;
            pw.pet.start_drag();
        }
        if p.dragging {
            pw.pet.x = cursor.0 - p.grab.0;
            pw.pet.y = cursor.1 - p.grab.1;
            let dt = (now - p.last_at).as_secs_f32();
            if dt > 0.0 {
                let inst = ((cursor.0 - p.last.0) / dt, (cursor.1 - p.last.1) / dt);
                p.vel = (p.vel.0 * 0.5 + inst.0 * 0.5, p.vel.1 * 0.5 + inst.1 * 0.5);
            }
            p.last = cursor;
            p.last_at = now;
        }
    } else {
        if p.dragging {
            pw.pet.throw(p.vel.0, p.vel.1);
        } else {
            pw.pet.on_click();
        }
        pw.press = None;
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.tray.is_none() {
            self.build_tray();
            check_available(self.proxy.clone());
        }
        if self.demo_on_start {
            self.demo_on_start = false;
            self.on_hook(el, demo_event(0));
        }
        self.scan_sessions(el);
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Hook(v) => self.on_hook(el, v),
            UserEvent::Menu(ev) => self.on_menu(el, ev),
            UserEvent::SessionsChanged => self.scan_sessions(el),
            UserEvent::Update(result) => self.on_update(el, result),
            UserEvent::Available(v) => self.on_available(v),
        }
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::MouseInput { .. }) {
            debug_log(format_args!("{id:?} {event:?}"));
        }
        let WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } = event else { return };
        let Some(pw) = self.pets.iter_mut().find(|pw| pw.window.id() == id) else { return };
        match button {
            MouseButton::Middle => Self::next_avatar(pw),
            MouseButton::Left if platform::alt_down() => Self::next_avatar(pw),
            MouseButton::Left => {
                let c = platform::cursor_pos();
                let now = Instant::now();
                pw.press = Some(Press {
                    start: c,
                    grab: (c.0 - pw.pet.x, c.1 - pw.pet.y),
                    at: now,
                    dragging: false,
                    last: c,
                    last_at: now,
                    vel: (0.0, 0.0),
                });
            }
            MouseButton::Right => pw.pet.toggle_sleep(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let frame = self.frame_interval().unwrap_or(EMPTY);
        if now >= self.last_tick + frame {
            self.tick(el, now);
        }
        // Recalcula: el cuadro pudo crear, dormir o despertar monitos.
        let frame = self.frame_interval().unwrap_or(EMPTY);
        el.set_control_flow(ControlFlow::WaitUntil(self.last_tick + frame));
    }
}
