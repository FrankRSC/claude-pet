//! Dibuja un monito (y su globo de texto) en un buffer ARGB premultiplicado.
//! Como u32 little-endian eso queda en BGRA, que es lo que esperan tanto el DIB
//! de Windows como un CGImage premultiplied-first de macOS.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

use crate::pet::{cell_px, size, Emote, Mode, Pet, Wall, KITS_SHOWN, MAX_CELL};
use crate::sprites::{Grid, Look, Pose, GRID};

/// Lado del buffer/ventana: alcanza para el tamaño más grande con aire para el
/// globo. El sprite va centrado; lo transparente deja pasar los clics.
pub const BUF: u32 = MAX_CELL * GRID as u32 * 3;

/// Distancia entre la esquina de la ventana y la del sprite.
pub fn off() -> f32 {
    (BUF as f32 - size()) / 2.0
}
const TEXT_PX: f32 = 15.0;
const INK: [u8; 3] = [40, 32, 30];

#[derive(Clone, Copy, Hash)]
enum Orient {
    Up,
    Ceiling,
    Wall(Wall),
}

pub struct Renderer {
    font: Option<Font>,
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
}

fn load_font() -> Option<Font> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            "C:\\Windows\\Fonts\\segoeuib.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
        ]
    } else {
        &[
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/Library/Fonts/Arial.ttf",
        ]
    };
    candidates.iter().find_map(|p| {
        let data = std::fs::read(p).ok()?;
        Font::from_bytes(data, FontSettings::default()).ok()
    })
}

/// Mezcla un color (no premultiplicado) con opacidad `a` sobre el píxel.
fn blend(px: &mut u32, [r, g, b]: [u8; 3], a: f32) {
    if a <= 0.0 {
        return;
    }
    if a >= 1.0 {
        *px = 0xff00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32;
        return;
    }
    let inv = 1.0 - a;
    let ch = |shift: u32, src: u8| {
        let dst = ((*px >> shift) & 0xff) as f32;
        ((src as f32 * a + dst * inv).round() as u32).min(255) << shift
    };
    *px = ch(24, 255) | ch(16, r) | ch(8, g) | ch(0, b);
}

fn fill(buf: &mut [u32], x0: i32, y0: i32, w: i32, h: i32, color: [u8; 3], a: f32) {
    let n = BUF as i32;
    for y in y0.max(0)..(y0 + h).min(n) {
        for x in x0.max(0)..(x0 + w).min(n) {
            blend(&mut buf[(y * n + x) as usize], color, a);
        }
    }
}

/// Rectángulo en el buffer: x, y, ancho, alto (en píxeles, con decimales).
type RectF = (f32, f32, f32, f32);

/// Dibuja la rejilla estirada a `rect`. Cada celda es un rectángulo con los
/// bordes redondeados al píxel, así el pixel art no se emborrona a ningún
/// tamaño. Sirve para el sprite aplastado y para las crías.
fn draw_grid(buf: &mut [u32], frame: &Grid, flip: bool, (x, y, w, h): RectF, alpha: f32) {
    let (cw, ch) = (w / GRID as f32, h / GRID as f32);
    for (gy, row) in frame.iter().enumerate() {
        let (y0, y1) = ((y + gy as f32 * ch).round() as i32, (y + (gy + 1) as f32 * ch).round() as i32);
        for (gx, cell) in row.iter().enumerate() {
            let Some(color) = *cell else { continue };
            let col = if flip { GRID - 1 - gx } else { gx };
            let (x0, x1) = ((x + col as f32 * cw).round() as i32, (x + (col + 1) as f32 * cw).round() as i32);
            fill(buf, x0, y0, x1 - x0, y1 - y0, color, alpha);
        }
    }
}

/// Dónde va el sprite parado con aplastamiento `s`: mismos pies, centrado.
fn sprite_rect(s: f32) -> RectF {
    let (w, h) = (size() * (1.0 + s * 0.5), size() * (1.0 - s));
    (off() + (size() - w) / 2.0, off() + size() - h, w, h)
}

/// El aplastamiento del cuadro: el de un salto o aterrizaje si lo hay; si no,
/// el rebote de cada paso y la respiración, que salen de la pose y del reloj
/// sin guardar nada.
fn squash_for(pet: &Pet, pose: Pose) -> f32 {
    if pet.squash != 0.0 {
        return pet.squash;
    }
    match (pet.mode, pose) {
        (Mode::Walking, Pose::WalkA) => 0.06,
        (Mode::Walking, Pose::WalkB) => -0.04,
        (Mode::Idle | Mode::Sitting, _) if pet.anim % 3.0 < 0.5 => -0.04,
        (Mode::Sleeping, _) if pet.anim % 4.0 < 2.0 => 0.05,
        _ => 0.0,
    }
}

/// Pixel art de 5×5 de cada emote y su color.
fn emote_art(e: Emote) -> ([&'static str; 5], [u8; 3]) {
    match e {
        Emote::Bang => (["..#..", "..#..", "..#..", ".....", "..#.."], [235, 70, 60]),
        Emote::Question => ([".###.", "#...#", "..##.", ".....", "..#.."], [250, 205, 60]),
        Emote::Heart => ([".#.#.", "#####", "#####", ".###.", "..#.."], [240, 100, 140]),
        Emote::Note => (["..##.", "..#.#", "..#..", "###..", "##..."], [110, 175, 245]),
        Emote::Sweat => (["..#..", ".###.", "#####", "#####", ".###."], [120, 190, 245]),
    }
}

/// El emote que se dibuja ahora y su paso de vaivén (0 o 1). Nada si hay
/// globo o si no está derecho.
fn emote_now(pet: &Pet, orient: Orient) -> Option<(Emote, i32)> {
    if pet.bubble.is_some() || !matches!(orient, Orient::Up) {
        return None;
    }
    let e = pet.emote()?;
    let step = match e {
        Emote::Heart | Emote::Note | Emote::Sweat => (pet.anim * 2.5) as i32 % 2,
        Emote::Bang | Emote::Question => 0,
    };
    Some((e, step))
}

/// Las crías se ven con el padre de pie y quieto en su lugar (no al volar,
/// trepar o irse).
fn kits_visible(pet: &Pet, orient: Orient) -> bool {
    !pet.kits.is_empty()
        && matches!(orient, Orient::Up)
        && !matches!(pet.mode, Mode::Dragged | Mode::Falling | Mode::Leaving)
}

/// Paso del brinco de las crías: el mismo que el del padre al caminar (−1 si
/// no camina), así no agregan cuadros.
fn kit_step(pet: &Pet) -> i32 {
    if pet.mode != Mode::Walking {
        return -1;
    }
    let step = if pet.busy() { 0.14 } else { 0.25 };
    (pet.anim / step) as i32 % 2
}

/// Primera fila con algo dibujado (la coronilla).
fn top_row(frame: &Grid) -> usize {
    frame.iter().position(|r| r.iter().any(Option::is_some)).unwrap_or(0)
}

fn draw_emote(buf: &mut [u32], e: Emote, step: i32, frame: &Grid, alpha: f32) {
    let (art, color) = emote_art(e);
    let px = cell_px().max(2) as i32;
    let head = off() + top_row(frame) as f32 * cell_px() as f32;
    // La gota resbala por un lado de la cabeza; lo demás flota arriba a la derecha.
    let (x, y) = match e {
        Emote::Sweat => (off() + size() * 0.12, head + (step * px) as f32),
        _ => (off() + size() * 0.68, head - 6.0 * px as f32 - (step * px) as f32),
    };
    let (x, y) = (x.round() as i32, y.round() as i32);
    let lit = || {
        art.iter()
            .enumerate()
            .flat_map(|(r, row)| row.bytes().enumerate().filter(|(_, b)| *b == b'#').map(move |(c, _)| (c as i32, r as i32)))
    };
    // Primero un borde oscuro de un píxel, para que se lea sobre cualquier fondo.
    for (c, r) in lit() {
        fill(buf, x + c * px - 1, y + r * px - 1, px + 2, px + 2, INK, alpha);
    }
    for (c, r) in lit() {
        fill(buf, x + c * px, y + r * px, px, px, color, alpha);
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer { font: load_font(), glyphs: HashMap::new() }
    }

    fn glyph(&mut self, c: char) -> Option<&(Metrics, Vec<u8>)> {
        let font = self.font.as_ref()?;
        Some(self.glyphs.entry(c).or_insert_with(|| font.rasterize(c, TEXT_PX)))
    }

    fn text_width(&mut self, text: &str) -> f32 {
        text.chars().filter_map(|c| self.glyph(c).map(|(m, _)| m.advance_width)).sum()
    }

    fn draw_text(&mut self, buf: &mut [u32], text: &str, x: f32, baseline: f32, color: [u8; 3], alpha: f32) {
        let mut pen = x;
        let n = BUF as i32;
        for c in text.chars() {
            let Some((m, bitmap)) = self.glyph(c) else { continue };
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - (m.height as i32 + m.ymin) as f32).round() as i32;
            for row in 0..m.height as i32 {
                for col in 0..m.width as i32 {
                    let (px, py) = (gx + col, gy + row);
                    if px < 0 || py < 0 || px >= n || py >= n {
                        continue;
                    }
                    let cov = bitmap[(row * m.width as i32 + col) as usize] as f32 / 255.0;
                    blend(&mut buf[(py * n + px) as usize], color, cov * alpha);
                }
            }
            pen += m.advance_width;
        }
    }

    fn fit(&mut self, text: &str, max: f32) -> String {
        if self.text_width(text) <= max {
            return text.to_string();
        }
        let mut s: String = text.to_string();
        while !s.is_empty() && self.text_width(&format!("{s}…")) > max {
            s.pop();
        }
        format!("{}…", s.trim_end())
    }

    /// Todo lo que decide cómo se ve el cuadro (no dónde está). Si no cambia,
    /// el cuadro anterior sirve y no hay que volver a dibujar.
    pub fn visual_key(pet: &Pet, room_above: bool) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let (pose, orient) = pick_pose(pet);
        (pose, orient, pet.facing_left(), room_above, cell_px()).hash(&mut h);
        ((pet.alpha * 255.0) as u8).hash(&mut h);
        ((squash_for(pet, pose) * 40.0).round() as i32).hash(&mut h);
        emote_now(pet, orient).hash(&mut h);
        if kits_visible(pet, orient) {
            (pet.kits.len(), kit_step(pet)).hash(&mut h);
        }
        match &pet.bubble {
            Some((text, _)) => text.hash(&mut h),
            None if pet.busy() => ((pet.anim * 4.0) as i32 % 3).hash(&mut h),
            None if pet.mode == Mode::Sleeping => ((pet.anim * 2.0).sin() * 3.0).round().to_bits().hash(&mut h),
            None => {}
        }
        h.finish()
    }

    /// `room_above`: si hay espacio arriba para el globo (si no, va abajo).
    pub fn draw(&mut self, pet: &Pet, look: &Look, buf: &mut [u32], room_above: bool) {
        buf.fill(0);
        let (pose, orient) = pick_pose(pet);
        let frame = look.frame(pose);
        // Los avatares asimétricos (dino, tiburón) miran hacia donde caminan.
        let flip = pet.facing_left() && matches!(orient, Orient::Up | Orient::Ceiling);
        let base = off() as i32;
        let upright = matches!(orient, Orient::Up);
        if kits_visible(pet, orient) {
            self.draw_kits(buf, pet, look);
        }
        if upright {
            draw_grid(buf, frame, flip, sprite_rect(squash_for(pet, pose)), pet.alpha);
        } else {
            for gy in 0..GRID {
                for gx in 0..GRID {
                    let Some(color) = frame[gy][gx] else { continue };
                    let gx = if flip { GRID - 1 - gx } else { gx };
                    let (dx, dy) = match orient {
                        Orient::Up | Orient::Ceiling => (gx, GRID - 1 - gy),
                        Orient::Wall(Wall::Right) => (gy, GRID - 1 - gx),
                        Orient::Wall(Wall::Left) => (GRID - 1 - gy, gx),
                    };
                    let c = cell_px() as i32;
                    fill(buf, base + dx as i32 * c, base + dy as i32 * c, c, c, color, pet.alpha);
                }
            }
        }

        // Todo lo que va "sobre la cabeza" se mide desde la coronilla, no desde
        // el borde del cuadro: el Monito es más bajito que la rejilla.
        let head = off() + top_row(frame) as f32 * cell_px() as f32;
        if let Some((text, _)) = &pet.bubble {
            let text = text.clone();
            let head = if upright { head } else { off() };
            self.bubble(buf, &text, room_above, pet.alpha, head);
        } else if pet.busy() && upright {
            // Tres puntitos que "piensan" sobre la cabeza.
            let active = (pet.anim * 4.0) as i32 % 3;
            let c = cell_px() as i32;
            let dot = (c * 3 / 2).max(4);
            for i in 0..3 {
                let a = if i == active { 1.0 } else { 0.35 };
                let x0 = base + c * (GRID as i32 / 2 - 4) + i * c * 3;
                fill(buf, x0, head as i32 - c * 2 - dot, dot, dot, look.accent, a * pet.alpha);
            }
        } else if pet.mode == Mode::Sleeping {
            let bob = (pet.anim * 2.0).sin() * 3.0;
            self.draw_text(buf, "z Z", off() + size() * 0.6, head - 2.0 + bob, INK, 0.8);
        }
        if let Some((e, step)) = emote_now(pet, orient) {
            draw_emote(buf, e, step, frame, pet.alpha);
        }
    }

    /// Las crías van detrás del padre (las dos primeras) y luego delante, a la
    /// mitad de su tamaño y sobre el mismo piso. Las que no caben completas en
    /// la ventana, o pasan de cuatro, se cuentan en un "+N" sobre la primera.
    fn draw_kits(&mut self, buf: &mut [u32], pet: &Pet, look: &Look) {
        let ks = size() * 0.5;
        let gap = ks * 0.1;
        let behind = if pet.facing_left() { 1.0 } else { -1.0 };
        let center = off() + size() / 2.0;
        let floor = off() + size();
        let step = kit_step(pet);
        let frame = look.frame(match pet.mode {
            Mode::Sitting => Pose::Sit,
            Mode::Sleeping => Pose::Sleep,
            _ => Pose::Stand,
        });
        let slots = [(behind, 0.0), (behind, 1.0), (-behind, 0.0), (-behind, 1.0)];
        let mut drawn = 0;
        let mut first = None;
        for (side, rank) in slots {
            if drawn == pet.kits.len().min(KITS_SHOWN) {
                break;
            }
            let cx = center + side * (size() / 2.0 + gap + ks / 2.0 + rank * (ks + gap));
            if cx - ks / 2.0 < 0.0 || cx + ks / 2.0 > BUF as f32 {
                continue;
            }
            let up = step >= 0 && (step + drawn as i32) % 2 == 1;
            let hop = if up { ks / GRID as f32 * 2.0 } else { 0.0 };
            draw_grid(buf, frame, pet.facing_left(), (cx - ks / 2.0, floor - ks - hop, ks, ks), pet.alpha);
            first.get_or_insert(cx);
            drawn += 1;
        }
        if pet.kits.len() > drawn {
            let text = format!("+{}", pet.kits.len() - drawn);
            let x = first.unwrap_or(center) - self.text_width(&text) / 2.0;
            self.draw_text(buf, &text, x, floor - ks - 6.0, INK, pet.alpha);
        }
    }

    /// `head`: altura de la coronilla en el buffer (el globo va justo arriba).
    fn bubble(&mut self, buf: &mut [u32], text: &str, above: bool, alpha: f32, head: f32) {
        let text = self.fit(text, BUF as f32 - 28.0);
        let tw = self.text_width(&text);
        let (bw, bh) = ((tw + 18.0).round() as i32, 26);
        let n = BUF as i32;
        let bx = (n / 2 - bw / 2).clamp(2, n - bw - 2);
        let by = if above { (head as i32 - bh - 8).max(2) } else { (off() + size()) as i32 + 8 };
        let white = [255, 255, 255];
        // Borde y relleno redondeados a mano (esquinas de 1 px recortadas).
        fill(buf, bx + 1, by, bw - 2, bh, INK, alpha);
        fill(buf, bx, by + 1, bw, bh - 2, INK, alpha);
        fill(buf, bx + 2, by + 1, bw - 4, bh - 2, white, alpha);
        fill(buf, bx + 1, by + 2, bw - 2, bh - 4, white, alpha);
        // Colita apuntando al monito.
        let cx = n / 2;
        for i in 0..5 {
            let (y, w) = if above { (by + bh + i, 9 - i * 2) } else { (by - 1 - i, 9 - i * 2) };
            fill(buf, cx - w / 2, y, w.max(1), 1, if i == 4 { INK } else { white }, alpha);
        }
        let baseline = by as f32 + bh as f32 / 2.0 + TEXT_PX * 0.35;
        self.draw_text(buf, &text, bx as f32 + 9.0, baseline, INK, alpha);
    }
}

fn pick_pose(pet: &Pet) -> (Pose, Orient) {
    let t = pet.anim;
    let step = if pet.busy() { 0.14 } else { 0.25 };
    let walk = if (t / step) as i32 % 2 == 0 { Pose::WalkA } else { Pose::WalkB };
    let alternate = |a: Pose, b: Pose| if (t / 0.3) as i32 % 2 == 0 { a } else { b };
    if pet.hover && matches!(pet.mode, Mode::Idle | Mode::Walking | Mode::Sitting) {
        return (Pose::Happy, Orient::Up);
    }
    match pet.mode {
        Mode::Walking => (walk, Orient::Up),
        Mode::Climbing(w) => (walk, Orient::Wall(w)),
        Mode::Ceiling => (walk, Orient::Ceiling),
        Mode::Falling | Mode::Dragged => (Pose::ArmsUp, Orient::Up),
        Mode::Celebrating => (Pose::Happy, Orient::Up),
        Mode::Attention | Mode::Leaving => (alternate(Pose::ArmsUp, Pose::Stand), Orient::Up),
        Mode::Sitting => (Pose::Sit, Orient::Up),
        Mode::Sleeping => (Pose::Sleep, Orient::Up),
        Mode::Idle => {
            let blink = t % 3.2 < 0.12;
            (if blink { Pose::Blink } else { Pose::Stand }, Orient::Up)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::pet::CLAUDE_ORANGE;
    use crate::sprites::{self, tests::encode_png};

    const SCREEN: Rect = Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 };

    fn pet(mode: Mode, anim: f32) -> Pet {
        let mut p = Pet::new("t".into(), "t".into(), "Claude".into(), None, SCREEN, 0);
        p.bubble = None;
        p.mode = mode;
        p.anim = anim;
        p
    }

    fn render(r: &mut Renderer, p: &Pet, look: &Look) -> Vec<u32> {
        let mut buf = vec![0u32; (BUF * BUF) as usize];
        r.draw(p, look, &mut buf, true);
        buf
    }

    /// (primera columna, última columna, última fila) con algo opaco.
    fn bounds(buf: &[u32]) -> (usize, usize, usize) {
        let n = BUF as usize;
        let opaque = |x: usize, y: usize| buf[y * n + x] >> 24 == 0xff;
        let cols: Vec<usize> = (0..n).filter(|&x| (0..n).any(|y| opaque(x, y))).collect();
        let bottom = (0..n).rev().find(|&y| (0..n).any(|x| opaque(x, y))).unwrap();
        (cols[0], *cols.last().unwrap(), bottom)
    }

    #[test]
    fn squash_widens_and_keeps_the_feet() {
        let mut r = Renderer::new();
        let look = sprites::look(0, CLAUDE_ORANGE);
        let stand = bounds(&render(&mut r, &pet(Mode::Idle, 1.0), &look));
        let mut landed = pet(Mode::Idle, 1.0);
        landed.squash = 0.3;
        let squashed = bounds(&render(&mut r, &landed, &look));
        assert!(squashed.1 - squashed.0 > stand.1 - stand.0, "más ancho al aterrizar");
        assert_eq!(squashed.2, stand.2, "los pies no se mueven");
        // Sin aplastar, el sprite ocupa exactamente su rectángulo de siempre.
        let c = cell_px() as usize;
        assert_eq!(stand.2 + 1, off() as usize + GRID * c);
    }

    #[test]
    fn kits_are_drawn_beside_and_counted() {
        let mut r = Renderer::new();
        let look = sprites::look(0, CLAUDE_ORANGE);
        let mut p = pet(Mode::Idle, 1.0);
        let alone = render(&mut r, &p, &look);
        let key = Renderer::visual_key(&p, true);
        p.add_kit(Some("a"), "Explore");
        p.bubble = None;
        assert_ne!(Renderer::visual_key(&p, true), key);
        let with_kit = render(&mut r, &p, &look);
        let (a, b) = (bounds(&alone), bounds(&with_kit));
        assert!(b.1 - b.0 > a.1 - a.0, "la cría ensancha lo dibujado");
        assert_eq!(a.2, b.2, "y pisa el mismo suelo");
        // Volando no se ven.
        p.mode = Mode::Falling;
        assert_eq!(bounds(&render(&mut r, &p, &look)).0, a.0);
    }

    #[test]
    fn emotes_change_the_frame() {
        let look = sprites::look(0, CLAUDE_ORANGE);
        let calm = pet(Mode::Idle, 1.0);
        let mut loving = pet(Mode::Idle, 1.0);
        loving.hover = true;
        assert_ne!(Renderer::visual_key(&calm, true), Renderer::visual_key(&loving, true));
        let mut r = Renderer::new();
        let a = render(&mut r, &pet(Mode::Attention, 0.5), &look);
        let red = a.iter().filter(|&&p| p & 0xffffff == 0xeb463c).count();
        assert!(red > 0, "el ! rojo está dibujado");
    }

    /// Escribe target/anim_preview.png con cada animación y emote, para el
    /// Monito y el primer avatar cargado (si hay).
    #[test]
    fn anim_preview() {
        crate::skins::reload();
        let mut r = Renderer::new();
        let shots: Vec<(&str, Box<dyn Fn() -> Pet>)> = vec![
            ("parado", Box::new(|| pet(Mode::Idle, 1.0))),
            ("respira", Box::new(|| pet(Mode::Idle, 0.2))),
            ("paso A", Box::new(|| pet(Mode::Walking, 0.0))),
            ("paso B", Box::new(|| pet(Mode::Walking, 0.3))),
            ("aterriza", Box::new(|| {
                let mut p = pet(Mode::Idle, 1.0);
                p.squash = 0.3;
                p
            })),
            ("despega", Box::new(|| {
                let mut p = pet(Mode::Idle, 1.0);
                p.squash = -0.15;
                p
            })),
            ("atención", Box::new(|| pet(Mode::Attention, 0.5))),
            ("festeja", Box::new(|| pet(Mode::Celebrating, 0.0))),
            ("cariño", Box::new(|| {
                let mut p = pet(Mode::Idle, 1.0);
                p.hover = true;
                p
            })),
            ("espera", Box::new(|| {
                let mut p = pet(Mode::Idle, 1.0);
                p.on_waiting();
                p.bubble = None;
                p
            })),
            ("suda", Box::new(|| {
                let mut p = pet(Mode::Walking, 0.0);
                p.busy_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(400));
                p
            })),
            ("duerme", Box::new(|| pet(Mode::Sleeping, 0.5))),
            ("3 crías", Box::new(|| {
                let mut p = pet(Mode::Walking, 0.0);
                for k in ["a", "b", "c"] {
                    p.add_kit(Some(k), "Explore");
                }
                p.bubble = None;
                p
            })),
            ("6 crías", Box::new(|| {
                let mut p = pet(Mode::Idle, 1.0);
                for k in ["a", "b", "c", "d", "e", "f"] {
                    p.add_kit(Some(k), "Plan");
                }
                p.bubble = None;
                p
            })),
        ];
        let avatars: Vec<usize> = if sprites::count() > 1 { vec![0, 1] } else { vec![0] };
        let (s, o) = (size() as usize, off() as usize);
        let (tw, th) = (s * 7 / 2, s * 2);
        let (x0, y0) = (o + s / 2 - tw / 2, o - s);
        let w = shots.len() * tw;
        let h = avatars.len() * th;
        let bg = [52u8, 55, 68];
        let mut rgb = vec![bg; w * h];
        for (row, &avatar) in avatars.iter().enumerate() {
            let look = sprites::look(avatar, CLAUDE_ORANGE);
            for (col, (_, make)) in shots.iter().enumerate() {
                let buf = render(&mut r, &make(), &look);
                for y in 0..th {
                    for x in 0..tw {
                        let p = buf[(y0 + y) * BUF as usize + x0 + x];
                        let a = (p >> 24) as f32 / 255.0;
                        let ch = |shift: u32, b: u8| ((p >> shift) & 0xff) as f32 + b as f32 * (1.0 - a);
                        let px = [ch(16, bg[0]) as u8, ch(8, bg[1]) as u8, ch(0, bg[2]) as u8];
                        rgb[(row * th + y) * w + col * tw + x] = px;
                    }
                }
            }
        }
        let png = encode_png(w, h, &rgb);
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/target/anim_preview.png"), png).unwrap();
    }
}
