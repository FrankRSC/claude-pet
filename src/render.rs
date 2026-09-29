//! Dibuja un monito (y su globo de texto) en un buffer ARGB premultiplicado.
//! Como u32 little-endian eso queda en BGRA, que es lo que esperan tanto el DIB
//! de Windows como un CGImage premultiplied-first de macOS.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

use crate::pet::{cell_px, size, Mode, Pet, Wall, MAX_CELL};
use crate::sprites::{Look, Pose, GRID};

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
        for gy in 0..GRID {
            for gx in 0..GRID {
                let Some(color) = frame[gy][gx] else { continue };
                let gx = if flip { GRID - 1 - gx } else { gx };
                let (dx, dy) = match orient {
                    Orient::Up => (gx, gy),
                    Orient::Ceiling => (gx, GRID - 1 - gy),
                    Orient::Wall(Wall::Right) => (gy, GRID - 1 - gx),
                    Orient::Wall(Wall::Left) => (GRID - 1 - gy, gx),
                };
                let c = cell_px() as i32;
                fill(buf, base + dx as i32 * c, base + dy as i32 * c, c, c, color, pet.alpha);
            }
        }

        let upright = matches!(orient, Orient::Up);
        if let Some((text, _)) = &pet.bubble {
            let text = text.clone();
            self.bubble(buf, &text, room_above, pet.alpha);
        } else if pet.busy() && upright {
            // Tres puntitos que "piensan" sobre la cabeza.
            let active = (pet.anim * 4.0) as i32 % 3;
            let c = cell_px() as i32;
            let dot = (c * 3 / 2).max(4);
            for i in 0..3 {
                let a = if i == active { 1.0 } else { 0.35 };
                fill(buf, base + c * 4 + i * c * 3, base - c, dot, dot, look.accent, a * pet.alpha);
            }
        } else if pet.mode == Mode::Sleeping {
            let bob = (pet.anim * 2.0).sin() * 3.0;
            self.draw_text(buf, "z Z", off() + size() * 0.6, off() + size() * 0.375 + bob, INK, 0.8);
        }
    }

    fn bubble(&mut self, buf: &mut [u32], text: &str, above: bool, alpha: f32) {
        let text = self.fit(text, BUF as f32 - 28.0);
        let tw = self.text_width(&text);
        let (bw, bh) = ((tw + 18.0).round() as i32, 26);
        let n = BUF as i32;
        let bx = (n / 2 - bw / 2).clamp(2, n - bw - 2);
        let by = if above { off() as i32 - bh - 6 } else { (off() + size()) as i32 + 8 };
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
