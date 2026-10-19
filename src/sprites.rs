//! Sprites: la rejilla es de `GRID`×`GRID` celdas y los pies tocan la última
//! fila. El Monito está dibujado a mano a 16×16 (`O` cuerpo, `o` sombra, `k`
//! ojos, `.` transparente) y se centra en la rejilla; los demás avatares vienen
//! de `skins.rs` y aquí se construyen sus poses.

use crate::skins::{self, Skin};

pub const GRID: usize = 24;
pub type Color = [u8; 3];
pub type Grid = [[Option<Color>; GRID]; GRID];

/// Lado del arte dibujado a mano del Monito.
pub const ART: usize = 16;
pub type Frame = [&'static str; ART];

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

/// Color de una celda del arte de 16×16, o None si es transparente.
pub fn art_cell(frame: &Frame, x: usize, y: usize, base: Color) -> Option<Color> {
    let shade = |f: f32| base.map(|c| (c as f32 * f) as u8);
    match frame[y].as_bytes()[x] {
        b'O' => Some(base),
        b'o' => Some(shade(0.72)),
        b'k' => Some([34, 24, 22]),
        _ => None,
    }
}

/// Lo mismo, en coordenadas de la rejilla: el arte va centrado y con los pies
/// en la última fila.
pub fn cell(frame: &Frame, x: usize, y: usize, base: Color) -> Option<Color> {
    let (ox, oy) = ((GRID - ART) / 2, GRID - ART);
    if x < ox || y < oy || x - ox >= ART {
        return None;
    }
    art_cell(frame, x - ox, y - oy, base)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Número de avatares: el Monito más los cargados de `skins.json`.
pub fn count() -> usize {
    skins::with(|s| s.len()) + 1
}

pub fn name(index: usize) -> String {
    match index {
        0 => "Monito".into(),
        i => skins::with(|s| s.get(i - 1).map_or_else(|| "?".into(), |k| k.name.clone())),
    }
}

/// Generación del Pokémon (por su número), para agrupar el menú. None para el
/// Monito y los avatares sin `id`.
pub fn generation(index: usize) -> Option<u32> {
    match index {
        0 => None,
        i => skins::with(|s| s.get(i - 1).and_then(|k| k.id).and_then(skins::generation)),
    }
}

/// El Monito se tiñe con el color de la sesión; los demás tienen paleta fija.
pub fn look(index: usize, tint: Color) -> Look {
    match index % count() {
        0 => monito(tint),
        i => skins::with(|s| build(&s[i - 1])),
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

fn build(skin: &Skin) -> Look {
    let mut base: Grid = [[None; GRID]; GRID];
    let mut eyes = [[false; GRID]; GRID];
    let mut counts: Vec<(Color, usize)> = Vec::new();
    for (y, row) in skin.rows.iter().enumerate().take(GRID) {
        for (x, &c) in row.iter().enumerate().take(GRID) {
            let Some(&(_, color)) = skin.palette.iter().find(|(k, _)| *k == c) else { continue };
            base[y][x] = Some(color);
            match counts.iter_mut().find(|(col, _)| *col == color) {
                Some((_, n)) => *n += 1,
                None => counts.push((color, 1)),
            }
        }
    }
    for &(x, y) in &skin.eyes {
        if x < GRID && y < GRID && base[y][x].is_some() {
            eyes[y][x] = true;
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

/// Ojos cerrados: de cada columna de celdas de ojo queda solo la de abajo (una
/// rayita); las demás, y los ojos de una sola celda, toman el color de al lado.
fn close_eyes(g: &Grid, eyes: &[[bool; GRID]; GRID]) -> Grid {
    let mut out = *g;
    for y in 0..GRID {
        for x in 0..GRID {
            if !eyes[y][x] {
                continue;
            }
            let above = y > 0 && eyes[y - 1][x];
            let below = y + 1 < GRID && eyes[y + 1][x];
            if above && !below {
                continue;
            }
            let neighbor = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)]
                .into_iter()
                .filter(|&(nx, ny)| nx < GRID && ny < GRID && !eyes[ny][nx])
                .find_map(|(nx, ny)| g[ny][nx]);
            if let Some(c) = neighbor {
                out[y][x] = Some(c);
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

/// Ícono RGBA (datos, ancho, alto) para la bandeja: la pose principal del
/// avatar recortada a lo que ocupa y metida en un cuadro, a 4 px por celda.
/// macOS lo escala a 18 pt de alto y Windows a su tamaño de bandeja, así que
/// lo que importa es que el sprite llene el cuadro.
pub fn icon_rgba(index: usize, tint: Color) -> (Vec<u8>, u32, u32) {
    const SCALE: usize = 4;
    let look = look(index, tint);
    let frame = look.frame(Pose::Stand);
    let (mut x0, mut y0, mut x1, mut y1) = (GRID, GRID, 0, 0);
    for (y, row) in frame.iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            if c.is_some() {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x0 > x1 {
        return (vec![0; 4], 1, 1);
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let side = w.max(h);
    let (ox, oy) = ((side - w) / 2, side - h); // centrado, con los pies abajo
    let px = side * SCALE;
    let mut out = vec![0u8; px * px * 4];
    for y in 0..h {
        for x in 0..w {
            let Some([r, g, b]) = frame[y0 + y][x0 + x] else { continue };
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let i = (((oy + y) * SCALE + dy) * px + (ox + x) * SCALE + dx) * 4;
                    out[i..i + 4].copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
    (out, px as u32, px as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(rows: &[&str], eyes: &[(usize, usize)]) -> Skin {
        let (ox, oy) = ((GRID - rows.len()) / 2, GRID - rows.len());
        let mut full = vec![vec![b'.'; GRID]; GRID];
        for (y, r) in rows.iter().enumerate() {
            full[oy + y][ox..ox + r.len()].copy_from_slice(r.as_bytes());
        }
        Skin {
            id: None,
            name: "t".into(),
            rows: full,
            palette: vec![(b'a', [200, 100, 50]), (b'k', [0, 0, 0])],
            eyes: eyes.iter().map(|&(x, y)| (ox + x, oy + y)).collect(),
        }
    }

    #[test]
    fn monito_stands_on_the_last_row_centered() {
        let look = monito(crate::pet::CLAUDE_ORANGE);
        let stand = look.frame(Pose::Stand);
        assert!(stand[GRID - 1].iter().any(Option::is_some), "pies en la última fila");
        assert!(stand[GRID - ART - 1].iter().all(Option::is_none), "nada arriba del arte");
        let left = stand.iter().flat_map(|r| r.iter().position(Option::is_some)).min().unwrap();
        let right = stand.iter().flat_map(|r| r.iter().rposition(Option::is_some)).max().unwrap();
        assert_eq!(left, GRID - 1 - right, "centrado");
    }

    #[test]
    fn eyes_close_into_a_line() {
        // Ojo de dos celdas: al cerrarlo queda la de abajo. Ojo de una: desaparece.
        let s = skin(&["aaaa", "akak", "akaa", "aaaa"], &[(1, 1), (1, 2), (3, 1)]);
        let look = build(&s);
        let (ox, oy) = ((GRID - 4) / 2, GRID - 4);
        let open = look.frame(Pose::Stand);
        let closed = look.frame(Pose::Blink);
        let body = Some([200, 100, 50]);
        let dark = Some([0, 0, 0]);
        assert_eq!(open[oy + 1][ox + 1], dark);
        assert_eq!(closed[oy + 1][ox + 1], body, "arriba se cierra");
        assert_eq!(closed[oy + 2][ox + 1], dark, "abajo queda la rayita");
        assert_eq!(closed[oy + 1][ox + 3], body, "el ojo de una celda desaparece");
        assert_eq!(look.accent, [200, 100, 50]);
        // El contorno rodea el sprite.
        assert_eq!(open[oy - 1][ox], Some(OUTLINE));
        assert_eq!(open[oy][ox - 1], Some(OUTLINE));
    }

    #[test]
    fn icon_is_a_square_filled_by_the_sprite() {
        let (rgba, w, h) = icon_rgba(0, crate::pet::CLAUDE_ORANGE);
        assert_eq!(w, h);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // El Monito mide 16 celdas de ancho: el cuadro es 16 × 4 px.
        assert_eq!(w, 64);
        let opaque = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize] == 255;
        let col_has = |x: u32| (0..h).any(|y| opaque(x, y));
        let row_has = |y: u32| (0..w).any(|x| opaque(x, y));
        assert!(row_has(h - 1) && col_has(0) && col_has(w - 1), "toca abajo y ambos lados");
        assert!(!row_has(0), "arriba queda aire (el Monito no es cuadrado)");
    }

    #[test]
    fn count_includes_monito_and_loaded_skins() {
        assert!(count() >= 1);
        assert_eq!(name(0), "Monito");
        assert_eq!(generation(0), None);
        let _ = look(0, [1, 2, 3]);
    }

    /// Escribe target/avatars.png con las poses de los primeros avatares y
    /// target/avatars_big.png con la pose principal de todos, para verlos.
    #[test]
    fn preview_sheet() {
        skins::reload();
        const SCALE: usize = 4;
        const PAD: usize = 8;
        let shown = count().min(40);
        let cell_px = GRID * SCALE + PAD;
        let (w, h) = (POSES * cell_px + PAD, shown * cell_px + PAD);
        let mut rgb = vec![[36u8, 38, 48]; w * h];
        for i in 0..shown {
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

        // Pose principal en grande, 10 por fila.
        const BIG: usize = 5;
        const COLS: usize = 10;
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
