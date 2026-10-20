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
use crate::{codex, installer, ipc, skins, sprites, updater};

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
    /// Cuándo se apretó el último clic suelto, para reconocer el doble clic.
    last_click: Option<Instant>,
}

struct Tray {
    icon: TrayIcon,
    demo: MenuId,
    hide: CheckMenuItem,
    notify: CheckMenuItem,
    /// [0] = "Aleatorio"; [i + 1] = avatar i.
    avatars: Vec<CheckMenuItem>,
    /// [i] = avatar i entra en el aleatorio.
    pool: Vec<CheckMenuItem>,
    /// "Toda la generación N": la mete o saca completa del aleatorio.
    pool_gens: Vec<(u32, CheckMenuItem)>,
    pool_all: MenuId,
    /// [0] = "Aleatorio"; [i + 1] = avatar i como ícono de la barra.
    icons: Vec<CheckMenuItem>,
    /// Mismo orden que `SIZES`.
    sizes: Vec<CheckMenuItem>,
    update: MenuItem,
    restart: MenuId,
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
    last_codex_scan: Instant,
    /// Sesiones que ya tuvieron monito; el escaneo no las revive.
    scanned: std::collections::HashSet<String>,
    tray: Option<Tray>,
    hidden: bool,
    demo_on_start: bool,
    demo_count: u32,
    /// None = cada sesión recibe uno distinto.
    avatar_choice: Option<usize>,
    /// Avatar que se dibuja en la bandeja; None = un Pokémon al azar al arrancar.
    icon_choice: Option<usize>,
    /// Avatares (por nombre) fuera del aleatorio.
    excluded: std::collections::HashSet<String>,
}

fn config_path() -> Option<std::path::PathBuf> {
    Some(dirs::data_local_dir()?.join("claude-pet").join("config.json"))
}

/// Tamaños del menú: (nombre, píxeles por celda).
const SIZES: [(&str, u32); 4] = [("Chico", 2), ("Normal", 3), ("Grande", 4), ("Enorme", 5)];
/// Con más avatares que esto, el menú Avatar se agrupa por generación.
const GROUP_FROM: usize = 30;

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

fn find_avatar(name: &str) -> Option<usize> {
    (0..sprites::count()).find(|&i| sprites::name(i) == name)
}

fn load_avatar_choice() -> Option<usize> {
    find_avatar(load_config()["avatar"].as_str()?)
}

fn load_icon_choice() -> Option<usize> {
    find_avatar(load_config()["icon"].as_str()?)
}

fn load_excluded() -> std::collections::HashSet<String> {
    load_config()["excluded"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn save_excluded(excluded: &std::collections::HashSet<String>) {
    let mut names: Vec<&String> = excluded.iter().collect();
    names.sort();
    save_config("excluded", serde_json::json!(names));
}

fn save_avatar_choice(choice: Option<usize>) {
    save_config("avatar", choice.map_or_else(|| "aleatorio".to_string(), sprites::name).into());
}

/// Avatar elegido a mano para cada proyecto (por nombre de carpeta).
fn load_project_avatar(label: &str) -> Option<usize> {
    find_avatar(load_config()["project_avatars"][label].as_str()?)
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
    load_config()["cell_px"].as_u64().map_or(pet::DEFAULT_CELL, |n| n as u32)
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
    let n = skins::reload();
    debug_log(format_args!("{n} avatares en skins.json"));

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
        last_codex_scan: now,
        scanned: Default::default(),
        tray: None,
        hidden: false,
        demo_on_start: demo,
        demo_count: 0,
        avatar_choice: load_avatar_choice(),
        icon_choice: load_icon_choice(),
        excluded: load_excluded(),
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

/// Nombre de la carpeta del proyecto; sin carpeta, el nombre del agente.
fn label_from_cwd(cwd: &str, agent: &str) -> String {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("");
    if name.is_empty() { agent.into() } else { name.into() }
}

/// Arranca una instancia nueva (que espera a que esta suelte el puerto) y
/// cierra esta. Al arrancar, la nueva vuelve a buscar las sesiones abiertas.
fn restart(el: &ActiveEventLoop) {
    debug_log(format_args!("reiniciando"));
    if let Ok(exe) = std::env::current_exe() {
        platform::spawn_detached(&exe, &["run", "--wait"]);
    }
    el.exit();
}

/// Los hooks de la TUI de Codex los corre el daemon `codex app-server`, que no
/// muere al cerrar la sesión: el monito se ata al proceso de Codex que trabaja
/// en esa carpeta. Si el hook ya viene de un cliente (`codex exec`), ese mismo.
fn codex_pid(hook_pid: Option<u32>, cwd: &str) -> Option<u32> {
    let clients = codex::clients();
    if hook_pid.is_some_and(|p| clients.iter().any(|c| c.pid == p)) {
        return hook_pid;
    }
    codex::client_in(&clients, cwd).or(hook_pid)
}

/// "codex" → "Codex". Un agente que no conocemos se muestra con mayúscula.
fn agent_display(name: &str) -> String {
    match installer::Agent::from_name(name) {
        Some(a) => a.short().into(),
        None => {
            let mut c = name.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

/// Un ítem con palomita por avatar (índice = avatar) dentro de `menu`. Con
/// pocos avatares van directo; con una Pokédex entera, en submenús por
/// generación, que se devuelven por si hay que ponerles cabecera.
fn avatar_items(menu: &Submenu, checked: impl Fn(usize) -> bool) -> (Vec<CheckMenuItem>, Vec<(u32, Submenu)>) {
    let n = sprites::count();
    let mut items = Vec::with_capacity(n);
    let mut gens: Vec<(u32, Submenu)> = Vec::new();
    for i in 0..n {
        let item = CheckMenuItem::new(sprites::name(i), true, checked(i), None);
        match sprites::generation(i).filter(|_| n > GROUP_FROM) {
            None => {
                let _ = menu.append(&item);
            }
            Some(g) => {
                let k = match gens.iter().position(|(k, _)| *k == g) {
                    Some(k) => k,
                    None => {
                        gens.push((g, Submenu::new(format!("Generación {g}"), true)));
                        gens.len() - 1
                    }
                };
                let _ = gens[k].1.append(&item);
            }
        }
        items.push(item);
    }
    for (_, sub) in &gens {
        let _ = menu.append(sub);
    }
    (items, gens)
}

/// Avatares de la generación `g`.
fn in_generation(g: u32) -> Vec<usize> {
    (0..sprites::count()).filter(|&i| sprites::generation(i) == Some(g)).collect()
}

fn tray_icon(index: usize) -> Option<Icon> {
    let (rgba, w, h) = sprites::icon_rgba(index, pet::CLAUDE_ORANGE);
    Icon::from_rgba(rgba, w, h).ok()
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
        let restart = MenuItem::new("Reiniciar Claude Pet", true, None);
        let quit = MenuItem::new("Salir", true, None);
        // Avatar: uno fijo para todos, o aleatorio.
        let avatar_menu = Submenu::new("Avatar", true);
        let random = CheckMenuItem::new("Aleatorio (uno por sesión)", true, self.avatar_choice.is_none(), None);
        let _ = avatar_menu.append(&random);
        let _ = avatar_menu.append(&PredefinedMenuItem::separator());
        let choice = self.avatar_choice;
        let (mut avatars, _) = avatar_items(&avatar_menu, |i| choice == Some(i));
        avatars.insert(0, random);

        // Grupo del aleatorio: qué avatares entran. Cada generación trae un
        // ítem para meterla o sacarla completa.
        let pool_menu = Submenu::new("Grupo del aleatorio", true);
        let all = MenuItem::new("Todos", true, None);
        let _ = pool_menu.append(&all);
        let _ = pool_menu.append(&PredefinedMenuItem::separator());
        let excluded = &self.excluded;
        let (pool, gens) = avatar_items(&pool_menu, |i| !excluded.contains(&sprites::name(i)));
        let mut pool_gens = Vec::new();
        for (g, sub) in &gens {
            let whole = in_generation(*g).into_iter().all(|i| pool[i].is_checked());
            let item = CheckMenuItem::new(format!("Toda la generación {g}"), true, whole, None);
            let _ = sub.prepend(&PredefinedMenuItem::separator());
            let _ = sub.prepend(&item);
            pool_gens.push((*g, item));
        }

        let icon_menu = Submenu::new("Ícono de la barra", true);
        let random_icon = CheckMenuItem::new("Aleatorio (uno al arrancar)", true, self.icon_choice.is_none(), None);
        let _ = icon_menu.append(&random_icon);
        let _ = icon_menu.append(&PredefinedMenuItem::separator());
        let icon_choice = self.icon_choice;
        let (mut icons, _) = avatar_items(&icon_menu, |i| icon_choice == Some(i));
        icons.insert(0, random_icon);

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
            &pool_menu,
            &icon_menu,
            &size_menu,
            &hide,
            &notify,
            &PredefinedMenuItem::separator(),
            &update,
            &restart,
            &quit,
        ]);
        let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Claude Pet");
        let shown = self.icon_choice.unwrap_or_else(|| self.random_icon());
        if let Some(icon) = tray_icon(shown) {
            builder = builder.with_icon(icon);
        }
        if let Ok(icon) = builder.build() {
            self.tray = Some(Tray {
                icon,
                demo: demo.id().clone(),
                hide,
                notify,
                avatars,
                pool,
                pool_gens,
                pool_all: all.id().clone(),
                icons,
                sizes,
                update,
                restart: restart.id().clone(),
                quit: quit.id().clone(),
            });
        }
    }

    /// Un Pokémon al azar del grupo para la bandeja (el Monito solo si no hay otro).
    fn random_icon(&self) -> usize {
        let pool: Vec<usize> = self.pool().into_iter().filter(|&i| i != 0).collect();
        if pool.is_empty() {
            return 0;
        }
        let seed = std::time::UNIX_EPOCH.elapsed().map_or(0, |d| d.as_nanos() as u64);
        pool[(hash(&seed.to_string()) % pool.len() as u64) as usize]
    }

    /// None = uno al azar en cada arranque.
    fn choose_icon(&mut self, choice: Option<usize>) {
        self.icon_choice = choice;
        save_config("icon", choice.map_or_else(|| "aleatorio".to_string(), sprites::name).into());
        let shown = choice.unwrap_or_else(|| self.random_icon());
        if let Some(tray) = &self.tray {
            for (k, item) in tray.icons.iter().enumerate() {
                item.set_checked(if k == 0 { choice.is_none() } else { choice == Some(k - 1) });
            }
            let _ = tray.icon.set_icon(tray_icon(shown));
        }
    }

    /// Avatares que entran en el aleatorio: los no excluidos (si no queda
    /// ninguno, todos).
    fn pool(&self) -> Vec<usize> {
        let pool: Vec<usize> = (0..sprites::count()).filter(|&i| !self.excluded.contains(&sprites::name(i))).collect();
        if pool.is_empty() { (0..sprites::count()).collect() } else { pool }
    }

    /// Mete (`on`) o saca del aleatorio los avatares `which`, lo guarda y
    /// actualiza las palomitas, incluidas las de "Toda la generación".
    fn set_in_pool(&mut self, which: &[usize], on: bool) {
        for &i in which {
            let name = sprites::name(i);
            if on {
                self.excluded.remove(&name);
            } else {
                self.excluded.insert(name);
            }
        }
        save_excluded(&self.excluded);
        if let Some(tray) = &self.tray {
            for &i in which {
                tray.pool[i].set_checked(on);
            }
            for (g, item) in &tray.pool_gens {
                item.set_checked(in_generation(*g).into_iter().all(|i| tray.pool[i].is_checked()));
            }
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

    /// El avatar fijo elegido, o uno del grupo que dependa de la sesión y que
    /// no repita ninguno de los monitos que ya están en pantalla (mientras alcancen).
    fn pick_avatar(&self, session: &str, label: &str) -> usize {
        if let Some(i) = load_project_avatar(label) {
            return i;
        }
        if let Some(i) = self.avatar_choice {
            return i;
        }
        let pool = self.pool();
        let start = (hash(session) % pool.len() as u64) as usize;
        (0..pool.len())
            .map(|k| pool[(start + k) % pool.len()])
            .find(|i| self.pets.iter().all(|pw| pw.avatar != *i))
            .unwrap_or(pool[start])
    }

    fn set_avatar(pw: &mut PetWindow, avatar: usize) {
        pw.avatar = avatar;
        pw.look = sprites::look(avatar, pw.pet.color);
    }

    /// Clic central o ⌥+clic: el siguiente avatar del grupo, solo para este
    /// monito. Se recuerda para su proyecto.
    fn next_avatar(&mut self, idx: usize) {
        let pool = self.pool();
        let cur = pool.iter().position(|&i| i == self.pets[idx].avatar);
        let next = pool[cur.map_or(0, |c| (c + 1) % pool.len())];
        let pw = &mut self.pets[idx];
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
        self.reassign_avatars();
    }

    /// Reparte avatares de nuevo, uno por uno, para que en modo aleatorio no se repitan.
    fn reassign_avatars(&mut self) {
        let pets = std::mem::take(&mut self.pets);
        for mut pw in pets {
            let avatar = self.pick_avatar(&pw.pet.session, &pw.pet.label);
            Self::set_avatar(&mut pw, avatar);
            self.pets.push(pw);
        }
    }

    /// `claude-pet skins` reemplazó skins.json: recargar, rehacer el menú y
    /// volver a repartir avatares (los índices pudieron cambiar).
    fn reload_skins(&mut self) {
        let n = skins::reload();
        debug_log(format_args!("skins.json recargado: {n} avatares"));
        self.avatar_choice = load_avatar_choice();
        self.icon_choice = load_icon_choice();
        if self.tray.is_some() {
            self.tray = None;
            self.build_tray();
        }
        self.reassign_avatars();
    }

    fn spawn_pet(
        &mut self,
        el: &ActiveEventLoop,
        session: String,
        label: String,
        agent: String,
        cwd: &str,
        pid: Option<u32>,
    ) -> Option<usize> {
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
        let mut pet = Pet::new(session, label, agent, pid, area, self.pets.len());
        pet.cwd = cwd.to_string();
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
            last_click: None,
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

    /// Sesiones de Codex abiertas sin monito (por ejemplo, reanudadas sin
    /// escribirles todavía). Solo mira el disco si hay un proceso de Codex sin monito.
    fn scan_codex(&mut self, el: &ActiveEventLoop) {
        let clients: Vec<_> = codex::clients()
            .into_iter()
            .filter(|c| !self.pets.iter().any(|pw| pw.pet.pid == Some(c.pid)))
            .collect();
        if clients.is_empty() {
            return;
        }
        for s in codex::open_sessions(&clients) {
            if self.scanned.contains(&s.id) || self.pets.iter().any(|pw| pw.pet.session == s.id) {
                continue;
            }
            debug_log(format_args!("sesión de Codex abierta: {} en {} (pid {})", s.id, s.cwd, s.pid));
            self.on_hook(
                el,
                serde_json::json!({
                    "hook_event_name": "SessionStart",
                    "session_id": s.id,
                    "cwd": s.cwd,
                    "pet_pid": s.pid,
                    "agent": "codex",
                }),
            );
        }
    }

    fn on_hook(&mut self, el: &ActiveEventLoop, v: Value) {
        match v["cmd"].as_str() {
            Some("quit") => return el.exit(),
            Some("restart") => return restart(el),
            Some("reload") => return self.reload_skins(),
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
        // Los hooks de versiones anteriores no mandan "agent": son de Claude Code.
        let agent = agent_display(v["agent"].as_str().unwrap_or("claude"));
        debug_log(format_args!("hook {agent} {event} {session} pid={pid:?}"));

        let idx = match self.pets.iter().position(|p| p.pet.session == session) {
            Some(i) => i,
            None if event == "SessionEnd" => return,
            None => {
                self.scanned.insert(session.to_string());
                let cwd = v["cwd"].as_str().unwrap_or("");
                let pid = if v["agent"] == "codex" { codex_pid(pid, cwd) } else { pid };
                debug_log(format_args!("monito nuevo: {session} ({agent}, {event}, pid={pid:?})"));
                let label = label_from_cwd(cwd, &agent);
                match self.spawn_pet(el, session.to_string(), label, agent, cwd, pid) {
                    Some(i) => i,
                    None => return,
                }
            }
        };
        let notifications = self.notifications_on();
        let pet = &mut self.pets[idx].pet;
        // El primer PID manda: los hooks de Codex llegan con el del daemon,
        // que no es el de la sesión.
        if pet.pid.is_none() {
            pet.pid = pid;
        }
        match event {
            "UserPromptSubmit" => pet.on_prompt(),
            "SubagentStart" => pet.add_kit(v["agent_id"].as_str(), v["agent_type"].as_str().unwrap_or("")),
            "SubagentStop" => pet.remove_kit(v["agent_id"].as_str()),
            "Stop" => {
                let took = pet.on_stop();
                if notifications {
                    let body = match took {
                        Some(d) => format!("{} terminó en {}", pet.label, fmt_duration(d)),
                        None => format!("{} terminó", pet.label),
                    };
                    notify(format!("✅ {} terminó", pet.agent), body);
                }
            }
            "Notification" => {
                let fallback = format!("{} necesita tu atención", pet.agent);
                let msg = v["message"].as_str().unwrap_or(&fallback);
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
        if event == "Stop" {
            self.cheer_around(idx);
        }
    }

    /// Festejo contagioso: los vecinos libres de un monito que terminó saltan con él.
    fn cheer_around(&mut self, idx: usize) {
        let (x, y) = (self.pets[idx].pet.x, self.pets[idx].pet.y);
        for (k, pw) in self.pets.iter_mut().enumerate() {
            if k != idx && (pw.pet.x - x).abs() < 500.0 && (pw.pet.y - y).abs() < 300.0 {
                pw.pet.cheer();
            }
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
            } else if let Some(k) = tray.icons.iter().position(|item| *item.id() == ev.id) {
                self.choose_icon(k.checked_sub(1));
            } else if let Some(k) = tray.pool.iter().position(|item| *item.id() == ev.id) {
                // El ítem ya cambió de estado solo; lo leemos.
                let on = tray.pool[k].is_checked();
                self.set_in_pool(&[k], on);
            } else if let Some(k) = tray.pool_gens.iter().position(|(_, item)| *item.id() == ev.id) {
                let (g, on) = (tray.pool_gens[k].0, tray.pool_gens[k].1.is_checked());
                self.set_in_pool(&in_generation(g), on);
            } else if ev.id == tray.pool_all {
                let all: Vec<usize> = (0..sprites::count()).collect();
                self.set_in_pool(&all, true);
            } else if let Some(k) = tray.sizes.iter().position(|item| *item.id() == ev.id) {
                self.choose_size(SIZES[k].1);
            } else if ev.id == tray.restart {
                restart(el);
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
            } else if p.squash.abs() > 0.02 {
                // Recuperándose de un aterrizaje: un cuarto de segundo con más cuadros.
                MOVING
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

        if now - self.last_codex_scan > Duration::from_secs(15) {
            self.last_codex_scan = now;
            self.scan_codex(el);
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
        // Se saludan al cruzarse (cada par una vez; antes de que se esquiven).
        for j in 1..self.pets.len() {
            let (left, right) = self.pets.split_at_mut(j);
            for a in left.iter_mut() {
                pet::meet(&mut a.pet, &mut right[0].pet);
            }
        }
        // Los que no hacen nada voltean a ver al que te está llamando.
        let calls: Vec<f32> = self.pets.iter().filter(|pw| pw.pet.mode == Mode::Attention).map(|pw| pw.pet.x).collect();
        if !calls.is_empty() {
            for pw in &mut self.pets {
                if matches!(pw.pet.mode, Mode::Idle | Mode::Sitting) && !pw.pet.hover {
                    let x = pw.pet.x;
                    let nearest = calls.iter().copied().min_by(|p, q| (p - x).abs().total_cmp(&(q - x).abs()));
                    if let Some(target) = nearest.filter(|t| (t - x).abs() > 1.0) {
                        pw.pet.face(target);
                    }
                }
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
pub fn debug_log(msg: std::fmt::Arguments) {
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

/// Trae al frente la terminal o el editor de la sesión del monito.
fn go_to_session(pet: &mut Pet) {
    let target = pet.pid.and_then(|pid| platform::focus_session(pid, &pet.cwd));
    debug_log(format_args!("ir a la sesión {} (pid {:?}): {target:?}", pet.session, pet.pid));
    if target.is_none() {
        pet.say("No encontré su ventana", 2.5);
    }
}

/// Clic corto = globo con info; doble clic = ir a la sesión; mantener o
/// mover = arrastrar; soltar = lanzar.
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
            // Doble clic, o un clic si te está llamando: a su sesión. Se mide
            // entre los momentos en que se apretó, que llegan sin retraso.
            let pressed = p.at;
            let double = pw.last_click.is_some_and(|t| pressed - t < Duration::from_millis(450));
            if double || pw.pet.mode == Mode::Attention {
                pw.last_click = None;
                go_to_session(&mut pw.pet);
            } else {
                pw.pet.on_click();
                pw.last_click = Some(pressed);
            }
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
        self.scan_codex(el);
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
        let Some(idx) = self.pets.iter().position(|pw| pw.window.id() == id) else { return };
        if button == MouseButton::Middle || (button == MouseButton::Left && platform::alt_down()) {
            return self.next_avatar(idx);
        }
        let pw = &mut self.pets[idx];
        match button {
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
