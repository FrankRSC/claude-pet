//! Máquina de estados y física de un monito. Coordenadas en píxeles físicos
//! del escritorio; (x, y) es la esquina superior izquierda del sprite.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::geom::{area_at, in_any, Floor, Rect};

/// Píxeles por celda del sprite de 16×16 (ajustable desde la bandeja).
static CELL_PX: AtomicU32 = AtomicU32::new(5);
pub const MIN_CELL: u32 = 3;
pub const MAX_CELL: u32 = 8;

pub fn cell_px() -> u32 {
    CELL_PX.load(Ordering::Relaxed)
}

pub fn set_cell_px(px: u32) {
    CELL_PX.store(px.clamp(MIN_CELL, MAX_CELL), Ordering::Relaxed);
}

/// Tamaño del sprite en pantalla.
pub fn size() -> f32 {
    (cell_px() * crate::sprites::GRID as u32) as f32
}
const GRAVITY: f32 = 1800.0;
/// Qué tan alto puede saltar a una ventana (px).
const JUMP_REACH: f32 = 520.0;

pub const CLAUDE_ORANGE: [u8; 3] = [217, 119, 87];
const PALETTE: [[u8; 3]; 7] = [
    CLAUDE_ORANGE,
    [97, 160, 235],
    [110, 190, 110],
    [175, 125, 225],
    [235, 140, 180],
    [230, 190, 70],
    [70, 190, 180],
];

/// Lo que el monito "ve" en cada cuadro.
pub struct World<'a> {
    pub areas: &'a [Rect],
    /// Bordes superiores de ventanas donde puede pararse.
    pub floors: &'a [Floor],
    pub cursor: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wall {
    Left,
    Right,
}

impl Wall {
    /// Dirección que se aleja de la pared.
    fn away(self) -> f32 {
        match self {
            Wall::Left => 1.0,
            Wall::Right => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Falling,
    Walking,
    Idle,
    Sitting,
    Sleeping,
    Climbing(Wall),
    Ceiling,
    Dragged,
    Celebrating,
    Attention,
    Leaving,
}

pub struct Pet {
    pub session: String,
    pub label: String,
    pub color: [u8; 3],
    pub pid: Option<u32>,
    pub x: f32,
    pub y: f32,
    vx: f32,
    vy: f32,
    pub mode: Mode,
    /// Dirección horizontal (-1 izquierda, 1 derecha).
    dir: f32,
    /// Al trepar: -1 sube, 1 baja.
    climb_dir: f32,
    timer: f32,
    /// Reloj de animación en segundos.
    pub anim: f32,
    pub busy_since: Option<Instant>,
    pub bubble: Option<(String, f32)>,
    /// Qué hacer al aterrizar (celebrar o pedir atención).
    pending: Option<Mode>,
    hops: u32,
    pub alpha: f32,
    /// Ventana sobre la que está parado (None = piso de la pantalla).
    perch: Option<u64>,
    /// Último borde izquierdo visto de esa ventana, para seguirla si se mueve.
    perch_left: f32,
    /// El cursor está encima (lo pone la app cada cuadro).
    pub hover: bool,
    /// Segundos que le quedan persiguiendo al cursor.
    chase: f32,
    rng: u64,
}

pub fn hash(s: &str) -> u64 {
    // FNV-1a: estable entre ejecuciones, así cada sesión conserva su color.
    s.bytes()
        .fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

pub fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

impl Pet {
    pub fn new(session: String, label: String, pid: Option<u32>, area: Rect, index: usize) -> Pet {
        let h = hash(&session);
        // El primer monito es naranja Claude; los demás toman color por sesión.
        let color = if index == 0 { CLAUDE_ORANGE } else { PALETTE[(h % PALETTE.len() as u64) as usize] };
        let mut pet = Pet {
            session,
            color,
            pid,
            x: 0.0,
            y: area.t,
            vx: 0.0,
            vy: 0.0,
            mode: Mode::Falling,
            dir: 1.0,
            climb_dir: -1.0,
            timer: 0.0,
            anim: 0.0,
            busy_since: None,
            bubble: Some((format!("¡Hola! {label}"), 3.5)),
            label,
            pending: None,
            hops: 0,
            alpha: 1.0,
            perch: None,
            perch_left: 0.0,
            hover: false,
            chase: 0.0,
            rng: h | 1,
        };
        pet.x = area.l + pet.rand() * (area.r - area.l - size()).max(0.0);
        pet.dir = if pet.rand() < 0.5 { -1.0 } else { 1.0 };
        pet
    }

    fn rand(&mut self) -> f32 {
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.rand() * (hi - lo)
    }

    pub fn facing_left(&self) -> bool {
        self.dir < 0.0
    }

    pub fn busy(&self) -> bool {
        self.busy_since.is_some()
    }

    pub fn dead(&self) -> bool {
        self.mode == Mode::Leaving && self.alpha <= 0.0
    }

    pub fn on_ground_mode(&self) -> bool {
        matches!(
            self.mode,
            Mode::Walking | Mode::Idle | Mode::Sitting | Mode::Sleeping | Mode::Attention | Mode::Celebrating
        )
    }

    /// Si está en el suelo cambia ya de modo; si no, se suelta y lo hace al aterrizar.
    fn react(&mut self, mode: Mode) {
        if self.mode == Mode::Leaving {
            return;
        }
        if self.on_ground_mode() {
            self.enter(mode);
        } else {
            self.pending = Some(mode);
            if self.mode != Mode::Dragged && self.mode != Mode::Falling {
                self.fall(0.0);
            }
        }
    }

    fn enter(&mut self, mode: Mode) {
        self.mode = mode;
        match mode {
            Mode::Celebrating => {
                self.hops = 3;
                self.vy = 0.0;
            }
            Mode::Attention => self.timer = 30.0,
            _ => {}
        }
    }

    fn fall(&mut self, vx: f32) {
        self.mode = Mode::Falling;
        self.perch = None;
        self.vx = vx;
        self.vy = 0.0;
    }

    fn start_walking(&mut self) {
        self.mode = Mode::Walking;
        self.timer = self.range(3.0, 8.0);
    }

    fn idle(&mut self, secs: f32) {
        self.mode = Mode::Idle;
        self.timer = secs;
    }

    pub fn say(&mut self, text: impl Into<String>, secs: f32) {
        self.bubble = Some((text.into(), secs));
    }

    // ---- eventos de Claude Code ----

    pub fn on_prompt(&mut self) {
        self.busy_since = Some(Instant::now());
        self.bubble = None;
        if matches!(self.mode, Mode::Attention | Mode::Sitting | Mode::Sleeping | Mode::Idle) {
            self.start_walking();
        }
    }

    /// Devuelve cuánto tardó la tarea.
    pub fn on_stop(&mut self) -> Option<Duration> {
        let took = self.busy_since.take().map(|t| t.elapsed());
        self.say("¡Listo!", 6.0);
        self.react(Mode::Celebrating);
        took
    }

    pub fn on_attention(&mut self, msg: &str) {
        self.say(msg, 10.0);
        self.react(Mode::Attention);
    }

    pub fn on_waiting(&mut self) {
        self.say("te espero…", 5.0);
    }

    pub fn on_end(&mut self) {
        if self.mode != Mode::Leaving {
            self.say("¡Adiós!", 2.0);
            self.mode = Mode::Leaving;
            self.timer = 1.5;
        }
    }

    // ---- mouse y otros monitos ----

    pub fn status(&self) -> String {
        match (self.mode, self.busy_since) {
            (Mode::Attention, _) => "necesita tu atención".into(),
            (_, Some(t)) => format!("trabajando… {}", fmt_duration(t.elapsed())),
            (Mode::Sleeping, _) => "zzz".into(),
            _ => "esperando".into(),
        }
    }

    pub fn on_click(&mut self) {
        let text = format!("{} · {}", self.label, self.status());
        self.say(text, 4.0);
        if self.mode == Mode::Sleeping {
            self.idle(1.0);
        }
    }

    pub fn toggle_sleep(&mut self) {
        if self.mode == Mode::Sleeping {
            self.idle(0.5);
        } else if self.on_ground_mode() {
            self.mode = Mode::Sleeping;
            self.timer = 60.0;
        }
    }

    pub fn start_drag(&mut self) {
        if self.mode != Mode::Leaving {
            if matches!(self.mode, Mode::Celebrating | Mode::Attention) {
                self.pending.get_or_insert(self.mode);
            }
            self.mode = Mode::Dragged;
            self.perch = None;
            self.chase = 0.0;
            self.say("¡Wiii!", 1.0);
        }
    }

    pub fn throw(&mut self, vx: f32, vy: f32) {
        if self.mode == Mode::Dragged {
            self.mode = Mode::Falling;
            self.vx = vx.clamp(-2500.0, 2500.0);
            self.vy = vy.clamp(-2500.0, 2500.0);
        }
    }

    /// Velocidad si va volando rápido (para chocar con otros monitos).
    /// Parado sobre una ventana (y no en el borde de la pantalla).
    pub fn perched(&self) -> bool {
        self.perch.is_some()
    }

    pub fn flying_velocity(&self) -> Option<(f32, f32)> {
        let fast = self.vx * self.vx + self.vy * self.vy > 450.0 * 450.0;
        (self.mode == Mode::Falling && fast).then_some((self.vx, self.vy))
    }

    /// Lo golpeó otro monito que venía volando.
    pub fn knock(&mut self, vx: f32, vy: f32) {
        if matches!(self.mode, Mode::Dragged | Mode::Leaving | Mode::Falling) {
            return;
        }
        self.mode = Mode::Falling;
        self.perch = None;
        self.vx = vx;
        self.vy = vy;
        self.say("¡Ay!", 1.2);
    }

    /// El que golpea pierde velocidad.
    pub fn damp(&mut self) {
        self.vx *= 0.5;
        self.vy *= 0.5;
    }

    /// Para no encimarse: si hay otro monito justo enfrente, se da la vuelta.
    pub fn avoid(&mut self, other_x: f32, other_y: f32) {
        if self.mode == Mode::Walking
            && self.chase <= 0.0
            && (self.y - other_y).abs() < 2.0
            && (other_x - self.x) * self.dir > 0.0
            && (other_x - self.x).abs() < size() * 0.6
        {
            self.dir = -self.dir;
        }
    }

    // ---- simulación ----

    /// Altura de los pies sobre lo que lo sostiene, siguiendo a la ventana si se
    /// movió. None si la ventana ya no está (se cerró, minimizó o la taparon).
    fn support(&mut self, w: &World, a: Rect) -> Option<f32> {
        let Some(id) = self.perch else { return Some(a.b) };
        let segs: Vec<&Floor> = w.floors.iter().filter(|f| f.id == id).collect();
        let first = segs.first()?;
        self.x += first.left - self.perch_left;
        self.perch_left = first.left;
        let cx = self.x + size() / 2.0;
        segs.iter().find(|f| cx >= f.x0 - 2.0 && cx <= f.x1 + 2.0).map(|f| f.y)
    }

    /// El tramo de ventana en el que está parado, si lo hay.
    fn perch_floor<'a>(&self, w: &'a World) -> Option<&'a Floor> {
        let id = self.perch?;
        let cx = self.x + size() / 2.0;
        w.floors.iter().find(|f| f.id == id && cx >= f.x0 - 2.0 && cx <= f.x1 + 2.0)
    }

    pub fn update(&mut self, dt: f32, w: &World) {
        self.anim += dt;
        if let Some((_, t)) = &mut self.bubble {
            *t -= dt;
            if *t <= 0.0 {
                self.bubble = None;
            }
        }
        let a = area_at(w.areas, self.x + size() / 2.0, self.y + size() / 2.0);
        self.timer -= dt;
        self.chase -= dt;

        // Todo lo que está parado necesita algo bajo los pies.
        let mut floor = a.b - size();
        if self.on_ground_mode() {
            match self.support(w, a) {
                Some(feet) => floor = feet - size(),
                None => {
                    if matches!(self.mode, Mode::Celebrating | Mode::Attention) {
                        self.pending = Some(self.mode);
                    }
                    self.say("¡Uy!", 1.0);
                    self.fall(0.0);
                }
            }
        }

        // Si le pasas el cursor encima se queda quieto y contento.
        if self.hover && matches!(self.mode, Mode::Walking | Mode::Idle) {
            if self.mode == Mode::Walking && self.bubble.is_none() {
                self.say("♥", 1.2);
            }
            self.idle(0.8);
        }

        match self.mode {
            Mode::Dragged => {}
            Mode::Falling => self.step_falling(dt, w, a),
            Mode::Walking => self.step_walking(dt, w, a, floor),
            Mode::Idle => {
                self.y = floor;
                if self.timer <= 0.0 && !self.try_jump(w) {
                    if self.rand() < 0.5 {
                        self.dir = -self.dir;
                    }
                    self.start_walking();
                }
            }
            Mode::Sitting => {
                self.y = floor;
                if self.timer <= 0.0 {
                    if !self.busy() && self.rand() < 0.4 {
                        self.mode = Mode::Sleeping;
                        self.timer = self.range(10.0, 25.0);
                    } else {
                        self.start_walking();
                    }
                }
            }
            Mode::Sleeping => {
                self.y = floor;
                if self.timer <= 0.0 || self.busy() {
                    self.idle(1.0);
                }
            }
            Mode::Climbing(wall) => self.step_climbing(dt, a, wall),
            Mode::Ceiling => self.step_ceiling(dt, a),
            Mode::Celebrating => {
                self.vy += GRAVITY * dt;
                self.y += self.vy * dt;
                if self.y >= floor {
                    self.y = floor;
                    self.vy = 0.0;
                    if self.hops > 0 {
                        self.hops -= 1;
                        self.vy = -520.0;
                    } else {
                        self.idle(2.0);
                    }
                }
            }
            Mode::Attention => {
                self.y = floor;
                if self.timer <= 0.0 {
                    self.idle(1.0);
                }
            }
            Mode::Leaving => {
                if self.timer <= 0.0 {
                    self.alpha -= dt / 1.2;
                }
            }
        }

        // Nunca debe quedar fuera de todas las pantallas.
        if self.mode != Mode::Dragged && !in_any(w.areas, self.x + size() / 2.0, self.y + size() / 2.0) {
            self.x = self.x.clamp(a.l, a.r - size());
            self.y = self.y.clamp(a.t, a.b - size());
        }
    }

    fn land(&mut self) {
        self.vx = 0.0;
        self.vy = 0.0;
        match self.pending.take() {
            Some(m) => self.enter(m),
            None => {
                let secs = self.range(0.5, 1.5);
                self.idle(secs);
            }
        }
    }

    fn step_falling(&mut self, dt: f32, w: &World, a: Rect) {
        let prev_feet = self.y + size();
        self.vy += GRAVITY * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        let mid = self.y + size() / 2.0;
        if self.x < a.l && !in_any(w.areas, a.l - 2.0, mid) {
            self.x = a.l;
            self.vx = self.vx.abs() * 0.4;
        }
        if self.x + size() > a.r && !in_any(w.areas, a.r + 2.0, mid) {
            self.x = a.r - size();
            self.vx = -self.vx.abs() * 0.4;
        }
        if self.y < a.t {
            self.y = a.t;
            self.vy = self.vy.abs() * 0.3;
        }

        // ¿Cruzó el borde de arriba de alguna ventana mientras bajaba?
        if self.vy > 0.0 {
            let (feet, cx) = (self.y + size(), self.x + size() / 2.0);
            let hit = w
                .floors
                .iter()
                .filter(|f| prev_feet <= f.y + 1.0 && feet >= f.y && cx >= f.x0 && cx <= f.x1)
                .min_by(|p, q| p.y.total_cmp(&q.y));
            if let Some(f) = hit {
                self.y = f.y - size();
                self.perch = Some(f.id);
                self.perch_left = f.left;
                self.land();
                return;
            }
        }

        if self.y + size() >= a.b {
            self.y = a.b - size();
            if self.vy > 700.0 {
                // Rebote si cae fuerte.
                self.vy = -self.vy * 0.35;
                self.vx *= 0.7;
            } else {
                self.land();
            }
        }
    }

    /// Salta a la parte de arriba de una ventana cercana. Devuelve si saltó.
    fn try_jump(&mut self, w: &World) -> bool {
        if self.busy() || self.rand() > 0.35 {
            return false;
        }
        let (feet, cx) = (self.y + size(), self.x + size() / 2.0);
        let targets: Vec<Floor> = w
            .floors
            .iter()
            .filter(|f| {
                let dx = if cx < f.x0 { f.x0 - cx } else if cx > f.x1 { cx - f.x1 } else { 0.0 };
                Some(f.id) != self.perch
                    && f.y < feet - 40.0
                    && feet - f.y < JUMP_REACH
                    && f.x1 - f.x0 >= size() * 1.2
                    && dx < 450.0
            })
            .copied()
            .collect();
        if targets.is_empty() {
            return false;
        }
        let f = targets[(self.rand() * targets.len() as f32) as usize % targets.len()];
        let tx = cx.clamp(f.x0 + size() * 0.6, f.x1 - size() * 0.6) + self.range(-30.0, 30.0);
        // Tiro parabólico: sube 50 px por encima del borde y cae sobre él.
        let over = 50.0;
        let vy = -(2.0 * GRAVITY * (feet - f.y + over)).sqrt();
        let flight = -vy / GRAVITY + (2.0 * over / GRAVITY).sqrt();
        self.mode = Mode::Falling;
        self.perch = None;
        self.vy = vy;
        self.vx = (tx - cx) / flight;
        if self.vx != 0.0 {
            self.dir = self.vx.signum();
        }
        true
    }

    fn step_walking(&mut self, dt: f32, w: &World, a: Rect, floor: f32) {
        // A veces persigue al cursor si está cerca.
        if self.chase > 0.0 {
            let dx = w.cursor.0 - (self.x + size() / 2.0);
            if dx.abs() < 20.0 {
                self.chase = 0.0;
                self.say("¡Te atrapé!", 1.5);
                self.idle(1.5);
                return;
            }
            self.dir = dx.signum();
        }
        let speed = match (self.busy(), self.chase > 0.0) {
            (_, true) => 150.0,
            (true, _) => 130.0,
            _ => 55.0,
        };
        self.x += self.dir * speed * dt;

        // Sobre una ventana: al llegar a la orilla se da la vuelta o se tira.
        if let Some(f) = self.perch_floor(w).copied() {
            self.y = floor;
            let cx = self.x + size() / 2.0;
            let past_edge = (self.dir > 0.0 && cx > f.x1 - 4.0) || (self.dir < 0.0 && cx < f.x0 + 4.0);
            if past_edge {
                if self.rand() < 0.55 {
                    self.dir = -self.dir;
                } else {
                    self.fall(self.dir * speed);
                    return;
                }
            }
        } else {
            if self.y < floor - 1.0 {
                // Pasó a un monitor con el piso más abajo.
                self.fall(self.dir * speed);
                return;
            }
            self.y = floor;
        }

        let mid = self.y + size() / 2.0;
        if self.x + size() > a.r && !in_any(w.areas, a.r + 2.0, mid) {
            self.x = a.r - size();
            self.hit_wall(Wall::Right);
        } else if self.x < a.l && !in_any(w.areas, a.l - 2.0, mid) {
            self.x = a.l;
            self.hit_wall(Wall::Left);
        }
        if self.mode == Mode::Walking && self.timer <= 0.0 {
            let near_cursor = (w.cursor.0 - self.x).abs() < 500.0 && (w.cursor.1 - self.y).abs() < 300.0;
            let r = self.rand();
            if near_cursor && self.perch.is_none() && r < 0.15 {
                self.chase = 4.0;
                self.timer = 4.0;
            } else if r < 0.4 && self.try_jump(w) {
            } else if self.busy() || r < 0.6 {
                if self.rand() < 0.3 {
                    self.dir = -self.dir;
                }
                self.start_walking();
            } else if r < 0.82 {
                let secs = self.range(2.0, 4.0);
                self.idle(secs);
            } else {
                self.mode = Mode::Sitting;
                self.timer = self.range(4.0, 10.0);
            }
        }
    }

    fn hit_wall(&mut self, wall: Wall) {
        self.chase = 0.0;
        if self.rand() < 0.4 {
            self.mode = Mode::Climbing(wall);
            self.perch = None;
            self.climb_dir = -1.0;
            self.timer = 1.0;
        } else {
            self.dir = wall.away();
        }
    }

    fn step_climbing(&mut self, dt: f32, a: Rect, wall: Wall) {
        let speed = if self.busy() { 80.0 } else { 45.0 };
        self.x = match wall {
            Wall::Left => a.l,
            Wall::Right => a.r - size(),
        };
        self.y += self.climb_dir * speed * dt;
        if self.y <= a.t {
            self.y = a.t;
            self.mode = Mode::Ceiling;
            self.dir = wall.away();
            self.timer = self.range(3.0, 8.0);
        } else if self.y >= a.b - size() {
            self.y = a.b - size();
            self.dir = wall.away();
            self.start_walking();
        } else if self.timer <= 0.0 {
            self.timer = 1.0;
            let r = self.rand();
            if r < 0.06 {
                self.fall(wall.away() * 120.0);
            } else if r < 0.12 {
                self.climb_dir = -self.climb_dir;
            }
        }
    }

    fn step_ceiling(&mut self, dt: f32, a: Rect) {
        self.y = a.t;
        self.x += self.dir * 45.0 * dt;
        if self.x + size() > a.r {
            self.x = a.r - size();
            self.mode = Mode::Climbing(Wall::Right);
            self.climb_dir = 1.0;
            self.timer = 1.0;
        } else if self.x < a.l {
            self.x = a.l;
            self.mode = Mode::Climbing(Wall::Left);
            self.climb_dir = 1.0;
            self.timer = 1.0;
        } else if self.timer <= 0.0 {
            if self.rand() < 0.5 {
                self.fall(self.dir * 40.0);
            } else {
                self.timer = self.range(2.0, 5.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 };

    fn world<'a>(floors: &'a [Floor]) -> World<'a> {
        World { areas: std::slice::from_ref(&SCREEN), floors, cursor: (-9999.0, -9999.0) }
    }

    fn run(pet: &mut Pet, w: &World, secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            pet.update(1.0 / 60.0, w);
        }
    }

    fn pet_at(x: f32, y: f32) -> Pet {
        let mut p = Pet::new("t".into(), "t".into(), None, SCREEN, 0);
        p.bubble = None;
        p.x = x;
        p.y = y;
        p
    }

    #[test]
    fn falls_onto_window_top_and_follows_it() {
        let mut floors = vec![Floor { id: 7, y: 500.0, x0: 200.0, x1: 1000.0, left: 200.0 }];
        let mut p = pet_at(500.0, 100.0);
        run(&mut p, &world(&floors), 1.0);
        assert_eq!(p.perch, Some(7));
        assert_eq!(p.y + size(), 500.0);

        // La ventana se mueve 100 px a la derecha y 50 abajo: el monito va con ella.
        p.idle(10.0);
        let before = p.x;
        floors[0] = Floor { id: 7, y: 550.0, x0: 300.0, x1: 1100.0, left: 300.0 };
        run(&mut p, &world(&floors), 0.1);
        assert!((p.x - before - 100.0).abs() < 0.01);
        assert_eq!(p.y + size(), 550.0);

        // Se cierra la ventana: cae al piso.
        run(&mut p, &world(&[]), 2.0);
        assert_eq!(p.perch, None);
        assert_eq!(p.y + size(), SCREEN.b);
    }

    #[test]
    fn jump_lands_on_target_window() {
        let floors = [Floor { id: 3, y: 700.0, x0: 600.0, x1: 1200.0, left: 600.0 }];
        let mut p = pet_at(400.0, SCREEN.b - size());
        p.idle(0.0);
        let w = world(&floors);
        let mut jumped = false;
        for _ in 0..50 {
            p.timer = 0.0;
            p.mode = Mode::Idle;
            if p.try_jump(&w) {
                jumped = true;
                break;
            }
        }
        assert!(jumped);
        run(&mut p, &w, 2.0);
        assert_eq!(p.perch, Some(3));
    }

    #[test]
    fn thrown_pet_knocks_another() {
        let mut other = pet_at(800.0, SCREEN.b - size());
        other.idle(5.0);
        other.knock(600.0, -300.0);
        assert_eq!(other.mode, Mode::Falling);
    }
}
