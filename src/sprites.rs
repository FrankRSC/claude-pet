//! Pixel art del monito, 16×16. `O` cuerpo, `o` sombra, `k` ojos, `.` transparente.
//! Los pies siempre tocan la fila 15. Aquí también se construyen las poses de
//! los avatares de `skins.rs`.

use crate::skins::{Skin, SKINS};

pub const GRID: usize = 16;
pub type Frame = [&'static str; GRID];
pub type Color = [u8; 3];
pub type Grid = [[Option<Color>; GRID]; GRID];

pub const STAND: Frame = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OOkOOOOOOkOO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "...o.o....o.o...",
    "...o.o....o.o...",
    "...o.o....o.o...",
];

pub const BLINK: Frame = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OkkOOOOOOkkO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "...o.o....o.o...",
    "...o.o....o.o...",
    "...o.o....o.o...",
];

pub const WALK_A: Frame = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OOkOOOOOOkOO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "..o...o..o...o..",
    "..o...o..o...o..",
    "..o...o..o...o..",
];

pub const WALK_B: Frame = [
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OOkOOOOOOkOO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "....oo....oo....",
    "....oo....oo....",
    "....oo....oo....",
    "....oo....oo....",
];

pub const ARMS_UP: Frame = [
    "................",
    "................",
    "................",
    "O..............O",
    "OO............OO",
    ".OOOOOOOOOOOOOO.",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OOkOOOOOOkOO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "..o...o..o...o..",
    "..o...o..o...o..",
    "..o...o..o...o..",
];

pub const HAPPY: Frame = [
    "................",
    "................",
    "................",
    "O..............O",
    "OO............OO",
    ".OOOOOOOOOOOOOO.",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OkOkOOOOkOkO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..oooooooooooo..",
    "..o...o..o...o..",
    "..o...o..o...o..",
    "..o...o..o...o..",
];

pub const SIT: Frame = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOkOOOOOOkOO..",
    "..OOkOOOOOOkOO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    ".oooooooooooooo.",
];

pub const SLEEP: Frame = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OOOOOOOOOOOO..",
    "..OkkOOOOOOkkO..",
    "OOOOOOOOOOOOOOOO",
    "OOOOOOOOOOOOOOOO",
    "..OOOOOOOOOOOO..",
    ".oooooooooooooo.",
];

/// Color de cada celda, o None si es transparente.
pub fn cell(frame: &Frame, x: usize, y: usize, base: [u8; 3]) -> Option<[u8; 3]> {
    let shade = |f: f32| base.map(|c| (c as f32 * f) as u8);
    match frame[y].as_bytes()[x] {
        b'O' => Some(base),
        b'o' => Some(shade(0.72)),
        b'k' => Some([34, 24, 22]),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Stand,
    Blink,
    WalkA,
    WalkB,
    ArmsUp,
    Happy,
    Sit,
    Sleep,
}

const POSES: usize = 8;
const OUTLINE: Color = [28, 20, 30];

/// Todas las poses ya resueltas a colores de un avatar.
pub struct Look {
    frames: [Grid; POSES],
    /// Color principal (para los puntitos de "trabajando").
    pub accent: Color,
}

impl Look {
    pub fn frame(&self, pose: Pose) -> &Grid {
        &self.frames[pose as usize]
    }
}

/// Número de avatares: el monito original más los de `skins.rs`.
pub fn count() -> usize {
    SKINS.len() + 1
}

pub fn name(index: usize) -> &'static str {
    match index {
        0 => "Monito",
        i => SKINS[i - 1].name,
    }
}

/// El monito original se tiñe con el color de la sesión; los demás tienen paleta fija.
pub fn look(index: usize, tint: Color) -> Look {
    match index % count() {
        0 => monito(tint),
        i => build(&SKINS[i - 1]),
    }
}

fn monito(tint: Color) -> Look {
    let grid = |f: &Frame| {
        let mut g: Grid = [[None; GRID]; GRID];
        for (y, row) in g.iter_mut().enumerate() {
            for (x, c) in row.iter_mut().enumerate() {
                *c = cell(f, x, y, tint);
            }
        }
        g
    };
    Look {
        frames: [&STAND, &BLINK, &WALK_A, &WALK_B, &ARMS_UP, &HAPPY, &SIT, &SLEEP].map(grid),
        accent: tint,
    }
}

/// Filas de 8 se reflejan para formar 16; las de 16 se usan tal cual.
fn expand(row: &str) -> Vec<u8> {
    let b = row.as_bytes();
    if b.len() == GRID / 2 {
        b.iter().chain(b.iter().rev()).copied().collect()
    } else {
        b.to_vec()
    }
}

fn build(skin: &Skin) -> Look {
    let mut base: Grid = [[None; GRID]; GRID];
    let mut eyes = [[false; GRID]; GRID];
    let mut counts: Vec<(Color, usize)> = Vec::new();
    for (y, row) in skin.rows.iter().enumerate() {
        for (x, &c) in expand(row).iter().enumerate().take(GRID) {
            let Some(&(_, color)) = skin.palette.iter().find(|(k, _)| *k == c) else { continue };
            base[y][x] = Some(color);
            eyes[y][x] = c == skin.eye;
            match counts.iter_mut().find(|(col, _)| *col == color) {
                Some((_, n)) => *n += 1,
                None => counts.push((color, 1)),
            }
        }
    }
    let closed = close_eyes(&base, &eyes);
    let accent = counts.iter().max_by_key(|(_, n)| *n).map_or([200, 200, 200], |(c, _)| *c);
    Look {
        frames: [
            outline(base),
            outline(closed),
            outline(base),
            outline(shift(&base, -1)),
            outline(shift(&base, -1)),
            outline(shift(&closed, -1)),
            outline(shift(&base, 2)),
            outline(shift(&closed, 2)),
        ],
        accent,
    }
}

/// Ojos cerrados: donde hay dos celdas de ojo apiladas, la de arriba toma el
/// color de su vecino y queda una rayita.
fn close_eyes(g: &Grid, eyes: &[[bool; GRID]; GRID]) -> Grid {
    let mut out = *g;
    for y in 0..GRID - 1 {
        for x in 0..GRID {
            if eyes[y][x] && eyes[y + 1][x] {
                let neighbor = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1))]
                    .into_iter()
                    .filter(|&(nx, ny)| nx < GRID && ny < GRID && !eyes[ny][nx])
                    .find_map(|(nx, ny)| g[ny][nx]);
                out[y][x] = neighbor;
            }
        }
    }
    out
}

/// Desplaza verticalmente (dy > 0 baja); lo que sale del cuadro se recorta.
fn shift(g: &Grid, dy: i32) -> Grid {
    let mut out: Grid = [[None; GRID]; GRID];
    for (y, row) in out.iter_mut().enumerate() {
        let src = y as i32 - dy;
        if (0..GRID as i32).contains(&src) {
            *row = g[src as usize];
        }
    }
    out
}

/// Contorno oscuro de una celda alrededor del sprite, para que se lea sobre cualquier fondo.
fn outline(g: Grid) -> Grid {
    let mut out = g;
    for y in 0..GRID {
        for x in 0..GRID {
            if g[y][x].is_some() {
                continue;
            }
            let near = [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                (0..GRID as i32).contains(&nx) && (0..GRID as i32).contains(&ny) && g[ny as usize][nx as usize].is_some()
            });
            if near {
                out[y][x] = Some(OUTLINE);
            }
        }
    }
    out
}

/// Ícono RGBA de 32×32 para la bandeja.
pub fn icon_rgba() -> Vec<u8> {
    const SIZE: usize = GRID * 2;
    let mut out = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            if let Some([r, g, b]) = cell(&STAND, x / 2, y / 2, crate::pet::CLAUDE_ORANGE) {
                let i = (y * SIZE + x) * 4;
                out[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_are_well_formed() {
        for skin in SKINS {
            for (y, row) in skin.rows.iter().enumerate() {
                assert!(
                    row.len() == GRID || row.len() == GRID / 2,
                    "{} fila {y}: {} caracteres",
                    skin.name,
                    row.len()
                );
                for c in row.bytes().filter(|&c| c != b'.') {
                    assert!(
                        skin.palette.iter().any(|(k, _)| *k == c),
                        "{} fila {y}: '{}' no está en la paleta",
                        skin.name,
                        c as char
                    );
                }
            }
        }
    }

    /// Escribe target/avatars.png con todas las poses de todos los avatares.
    #[test]
    fn preview_sheet() {
        const SCALE: usize = 4;
        const PAD: usize = 8;
        let cell_px = GRID * SCALE + PAD;
        let (w, h) = (POSES * cell_px + PAD, count() * cell_px + PAD);
        let mut rgb = vec![[36u8, 38, 48]; w * h];
        for i in 0..count() {
            let look = look(i, crate::pet::CLAUDE_ORANGE);
            for (p, frame) in look.frames.iter().enumerate() {
                let (ox, oy) = (PAD + p * cell_px, PAD + i * cell_px);
                for y in 0..GRID * SCALE {
                    for x in 0..GRID * SCALE {
                        let c = frame[y / SCALE][x / SCALE].unwrap_or([52, 55, 68]);
                        rgb[(oy + y) * w + ox + x] = c;
                    }
                }
            }
        }
        let png = encode_png(w, h, &rgb);
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/target/avatars.png"), png).unwrap();

        // Pose principal en grande, 6 por fila.
        const BIG: usize = 8;
        const COLS: usize = 6;
        let cell_px = GRID * BIG + PAD;
        let rows = count().div_ceil(COLS);
        let (w, h) = (COLS * cell_px + PAD, rows * cell_px + PAD);
        let mut rgb = vec![[36u8, 38, 48]; w * h];
        for i in 0..count() {
            let look = look(i, crate::pet::CLAUDE_ORANGE);
            let (ox, oy) = (PAD + (i % COLS) * cell_px, PAD + (i / COLS) * cell_px);
            for y in 0..GRID * BIG {
                for x in 0..GRID * BIG {
                    let c = look.frame(Pose::Stand)[y / BIG][x / BIG].unwrap_or([52, 55, 68]);
                    rgb[(oy + y) * w + ox + x] = c;
                }
            }
        }
        let png = encode_png(w, h, &rgb);
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/target/avatars_big.png"), png).unwrap();
    }

    /// PNG mínimo sin compresión (bloques "stored" de deflate).
    fn encode_png(w: usize, h: usize, rgb: &[[u8; 3]]) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut c = 0xffff_ffffu32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend(data);
            out.extend(&body);
            out.extend(crc32(&body).to_be_bytes());
        }
        let mut raw = Vec::with_capacity(h * (w * 3 + 1));
        for y in 0..h {
            raw.push(0);
            for px in &rgb[y * w..(y + 1) * w] {
                raw.extend(px);
            }
        }
        let mut z = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
        for (i, block) in blocks.iter().enumerate() {
            z.push((i == blocks.len() - 1) as u8);
            let len = block.len() as u16;
            z.extend(len.to_le_bytes());
            z.extend((!len).to_le_bytes());
            z.extend(*block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in &raw {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        z.extend(((b << 16) | a).to_be_bytes());

        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend((w as u32).to_be_bytes());
        ihdr.extend((h as u32).to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    }
}
