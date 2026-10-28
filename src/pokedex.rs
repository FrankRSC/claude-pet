//! Avatares de la Pokédex bajados en la máquina de cada quien: lo mismo que
//! `tools/pokedex.py`, pero sin Python, para que `install` los deje listos.
//! Los sprites son de Nintendo / Game Freak / Creatures y PokeAPI los da para
//! uso personal, así que no viajan en el binario ni en el repo.
//!
//! Por cada Pokémon baja el icono de menú de Generación VII (40×30), lo
//! recorta, lo reduce solo si no cabe en la rejilla, deja a lo más `COLORS`
//! colores, busca los ojos y baja su grito en Ogg Vorbis a `cries/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::sprites::GRID;
use crate::updater::curl;

const ICON: &str = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/versions/generation-vii/icons/";
const CRY: &str = "https://raw.githubusercontent.com/PokeAPI/cries/main/cries/pokemon/";
const GRAPHQL: &str = "https://graphql.pokeapi.co/v1beta2";
/// Los iconos de Generación VII llegan hasta Melmetal; de la 8 en adelante no hay.
pub const LAST: u32 = 809;
const COLORS: usize = 12;
const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Descargas por llamada a curl (reusa la conexión; en Windows la línea de
/// comandos no puede pasar de 32K caracteres) y llamadas en paralelo.
const BATCH: usize = 40;
const WORKERS: usize = 8;

type Rgb = [u8; 3];

/// `install` y `claude-pet pokemon`: los baja contando en la consola.
pub fn install() {
    match download(|msg| println!("{msg}")) {
        Ok((skins, cries)) => {
            crate::app::save_config("pokemon", true.into());
            println!("{skins} Pokémon y {cries} gritos listos; están en el menú Avatar.")
        }
        Err(e) => eprintln!("{e}. La app lo vuelve a intentar al arrancar, o corre `claude-pet pokemon`."),
    }
}

/// Baja los Pokémon 1..=`LAST` y escribe `skins.json` y `cries/` en la
/// carpeta de datos. Devuelve (avatares, gritos).
pub fn download(log: impl Fn(&str)) -> Result<(usize, usize), String> {
    let dir = crate::skins::dir().ok_or("No encuentro la carpeta de datos")?;
    let tmp = dir.join("pokedex-tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("No pude crear {}: {e}", tmp.display()))?;
    let result = download_into(&dir, &tmp, &log);
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn download_into(dir: &Path, tmp: &Path, log: &dyn Fn(&str)) -> Result<(usize, usize), String> {
    let ids: Vec<u32> = (1..=LAST).collect();

    log(&format!("Bajando {} Pokémon de PokeAPI…", ids.len()));
    let icon = |id: u32| tmp.join(format!("{id}.png"));
    fetch(ids.iter().map(|&id| (format!("{ICON}{id}.png"), icon(id))).collect());
    let names = spanish_names();

    let mut skins = Vec::new();
    for &id in &ids {
        let Some(img) = std::fs::read(icon(id)).ok().and_then(|b| decode(&b)) else { continue };
        let Some((rows, palette)) = pixelate(&img, GRID, COLORS) else { continue };
        let name = names.get(&id).cloned().unwrap_or_else(|| format!("#{id}"));
        let mut pal = Map::new();
        for (ch, c) in &palette {
            pal.insert((*ch as char).to_string(), json!(c));
        }
        // Los iconos de menú miran a la izquierda; la app los voltea al cargar.
        let mut skin = json!({ "id": id, "name": name, "facing": "left", "rows": rows, "palette": pal });
        let eyes = find_eyes(&rows, &palette);
        if !eyes.is_empty() {
            skin["eyes"] = json!(eyes.iter().map(|&(x, y)| [x, y]).collect::<Vec<_>>());
        }
        skins.push(skin);
    }
    if skins.is_empty() {
        return Err("No pude bajar ningún Pokémon (¿hay internet?)".into());
    }

    // Gritos: "latest" son los de los juegos nuevos, pero algunos son MP3 con
    // extensión .ogg (Pikachu) y la app solo decodifica Vorbis; para esos y
    // los que falten se prueba "legacy", los originales.
    log("Bajando sus gritos…");
    let cry = |kind: &str, id: u32| tmp.join(format!("{kind}-{id}.ogg"));
    fetch(ids.iter().map(|&id| (format!("{CRY}latest/{id}.ogg"), cry("latest", id))).collect());
    let missing: Vec<u32> = ids.iter().copied().filter(|&id| !is_vorbis(&cry("latest", id))).collect();
    fetch(missing.iter().map(|&id| (format!("{CRY}legacy/{id}.ogg"), cry("legacy", id))).collect());
    let cries_dir = dir.join("cries");
    let _ = std::fs::create_dir_all(&cries_dir);
    let mut cries = 0;
    for &id in &ids {
        let src = [cry("latest", id), cry("legacy", id)].into_iter().find(|p| is_vorbis(p));
        if src.is_some_and(|p| std::fs::copy(p, cries_dir.join(format!("{id}.ogg"))).is_ok()) {
            cries += 1;
        }
    }

    let text = serde_json::to_string(&json!({ "grid": GRID, "skins": skins })).unwrap() + "\n";
    let (loaded, _) = crate::skins::parse(&text)?;
    let path = crate::skins::path().ok_or("No encuentro la carpeta de datos")?;
    std::fs::write(&path, text).map_err(|e| format!("No pude escribir {}: {e}", path.display()))?;
    Ok((loaded.len(), cries))
}

/// Baja cada URL a su archivo, en paralelo. Las que fallan no dejan archivo.
fn fetch(jobs: Vec<(String, PathBuf)>) {
    let batches = Mutex::new(jobs.chunks(BATCH).map(<[_]>::to_vec).collect::<Vec<_>>());
    std::thread::scope(|s| {
        for _ in 0..WORKERS {
            s.spawn(|| loop {
                let Some(batch) = batches.lock().unwrap().pop() else { return };
                let mut cmd = curl();
                for (url, path) in &batch {
                    cmd.arg(url).arg("-o").arg(path);
                }
                let _ = cmd.stdout(std::process::Stdio::null()).status();
            });
        }
    });
}

fn is_vorbis(path: &Path) -> bool {
    let Ok(data) = std::fs::read(path) else { return false };
    data.starts_with(b"OggS") && data.windows(6).take(64).any(|w| w == b"vorbis")
}

/// Nombres en español (o en inglés si no hay) en una sola consulta a la API
/// GraphQL de PokeAPI. Sin red, vacío: quedan como "#25".
fn spanish_names() -> HashMap<u32, String> {
    // language_id 7 es español; 9, inglés.
    let query = json!({ "query": format!(
        "{{ pokemonspeciesname(where: {{ language_id: {{ _in: [7, 9] }}, pokemon_species_id: {{ _lte: {LAST} }} }}) {{ pokemon_species_id language_id name }} }}"
    ) });
    let Ok(out) = curl().args(["-H", "Content-Type: application/json", "-d", &query.to_string(), GRAPHQL]).output() else {
        return HashMap::new();
    };
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let mut names = HashMap::new();
    for n in v["data"]["pokemonspeciesname"].as_array().into_iter().flatten() {
        let (Some(id), Some(lang), Some(name)) = (n["pokemon_species_id"].as_u64(), n["language_id"].as_u64(), n["name"].as_str()) else {
            continue;
        };
        if lang == 7 || !names.contains_key(&(id as u32)) {
            names.insert(id as u32, name.to_string());
        }
    }
    names
}

/// Imagen RGBA: ancho, alto y un [r, g, b, a] por píxel.
struct Image {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>,
}

fn decode(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(bytes);
    // Paletas e índices a RGB(A) de 8 bits.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let px = match info.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Image { w: info.width as usize, h: info.height as usize, px })
}

/// Filas de `grid` caracteres (`.` transparente, pies en la última fila) y la
/// paleta en orden de aparición. None si el icono está vacío.
fn pixelate(img: &Image, grid: usize, colors: usize) -> Option<(Vec<String>, Vec<(u8, Rgb)>)> {
    // Recorte a lo que no es transparente.
    let opaque = |x: usize, y: usize| img.px[y * img.w + x][3] > 0;
    let xs: Vec<usize> = (0..img.w).filter(|&x| (0..img.h).any(|y| opaque(x, y))).collect();
    let ys: Vec<usize> = (0..img.h).filter(|&y| (0..img.w).any(|x| opaque(x, y))).collect();
    let (x0, y0) = (*xs.first()?, *ys.first()?);
    let (w, h) = (xs.last()? - x0 + 1, ys.last()? - y0 + 1);

    // Nunca ampliar: si cabe, va a tamaño real.
    let s = (grid as f64 / w.max(h) as f64).min(1.0);
    let nw = ((w as f64 * s).round_ties_even() as usize).max(1);
    let nh = ((h as f64 * s).round_ties_even() as usize).max(1);
    let at = |x: usize, y: usize| img.px[(y0 + y) * img.w + x0 + x];
    let cells = quantize(downscale(&at, w, h, nw, nh), colors);

    let (ox, oy) = ((grid - nw) / 2, grid - nh); // centrado, pies abajo
    let mut rows = vec![vec![b'.'; grid]; grid];
    let mut palette: Vec<(u8, Rgb)> = Vec::new();
    for (y, row) in cells.iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            let Some(c) = c else { continue };
            let ch = match palette.iter().find(|(_, p)| p == c) {
                Some((ch, _)) => *ch,
                None => {
                    let ch = CHARS[palette.len()];
                    palette.push((ch, *c));
                    ch
                }
            };
            rows[oy + y][ox + x] = ch;
        }
    }
    let rows = rows.into_iter().map(|r| String::from_utf8(r).unwrap()).collect();
    Some((rows, palette))
}

/// Promedio por bloque ponderado por alfa (evita bordes oscuros). None si el
/// bloque es mayormente transparente.
fn downscale(at: &dyn Fn(usize, usize) -> [u8; 4], w: usize, h: usize, nw: usize, nh: usize) -> Vec<Vec<Option<Rgb>>> {
    (0..nh)
        .map(|ty| {
            let (y0, y1) = (ty * h / nh, (ty * h / nh + 1).max((ty + 1) * h / nh));
            (0..nw)
                .map(|tx| {
                    let (x0, x1) = (tx * w / nw, (tx * w / nw + 1).max((tx + 1) * w / nw));
                    let (mut rgb, mut a, mut n) = ([0u64; 3], 0u64, 0u64);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let p = at(x, y);
                            let pa = p[3] as u64;
                            for i in 0..3 {
                                rgb[i] += p[i] as u64 * pa;
                            }
                            a += pa;
                            n += 1;
                        }
                    }
                    (a > 0 && a >= 128 * n).then(|| rgb.map(|c| (c / a) as u8))
                })
                .collect()
        })
        .collect()
}

/// Reduce los colores opacos a `colors` con median cut, sin dither: parte la
/// caja con más rango en su canal más largo, por la mediana, hasta tener
/// `colors` cajas, y cada píxel toma el promedio de la suya.
fn quantize(mut cells: Vec<Vec<Option<Rgb>>>, colors: usize) -> Vec<Vec<Option<Rgb>>> {
    let mut distinct: Vec<Rgb> = cells.iter().flatten().flatten().copied().collect();
    distinct.sort();
    distinct.dedup();
    if distinct.len() <= colors {
        return cells;
    }
    let mut boxes: Vec<Vec<Rgb>> = vec![cells.iter().flatten().flatten().copied().collect()];
    let range = |b: &[Rgb], i: usize| b.iter().map(|c| c[i]).max().unwrap() - b.iter().map(|c| c[i]).min().unwrap();
    let widest = |b: &[Rgb]| (0..3).max_by_key(|&i| range(b, i)).unwrap();
    while boxes.len() < colors {
        let Some(k) = (0..boxes.len()).filter(|&k| range(&boxes[k], widest(&boxes[k])) > 0).max_by_key(|&k| range(&boxes[k], widest(&boxes[k])))
        else {
            break;
        };
        let mut b = boxes.swap_remove(k);
        let ch = widest(&b);
        b.sort_by_key(|c| c[ch]);
        // Cortar en la mediana, sin dejar el mismo valor a ambos lados.
        let mid = b[b.len() / 2][ch];
        let cut = b.iter().position(|c| c[ch] >= mid).filter(|&i| i > 0).unwrap_or_else(|| b.iter().position(|c| c[ch] > mid).unwrap());
        let hi = b.split_off(cut);
        boxes.push(b);
        boxes.push(hi);
    }
    let mut map: HashMap<Rgb, Rgb> = HashMap::new();
    for b in &boxes {
        let n = b.len() as u64;
        let sum = b.iter().fold([0u64; 3], |s, c| [s[0] + c[0] as u64, s[1] + c[1] as u64, s[2] + c[2] as u64]);
        let mean = sum.map(|s| ((s + n / 2) / n) as u8);
        for c in b {
            map.insert(*c, mean);
        }
    }
    for c in cells.iter_mut().flatten().flatten() {
        *c = map[c];
    }
    cells
}

/// Celdas (x, y) de los ojos, para cerrarlos al parpadear: manchas de hasta
/// 2×2 del color más oscuro, en los dos tercios de arriba del sprite y
/// rodeadas de cuerpo (el contorno toca lo transparente, así que no cuenta).
/// Si no salen uno o dos ojos, nada.
fn find_eyes(rows: &[String], palette: &[(u8, Rgb)]) -> Vec<(usize, usize)> {
    let luma = |c: &Rgb| 0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64;
    let Some(&(dark, _)) = palette.iter().min_by(|a, b| luma(&a.1).total_cmp(&luma(&b.1))) else { return Vec::new() };
    let rows: Vec<&[u8]> = rows.iter().map(|r| r.as_bytes()).collect();
    let g = rows.len() as i64;
    let used: Vec<usize> = (0..rows.len()).filter(|&y| rows[y].iter().any(|&c| c != b'.')).collect();
    let (Some(&top), Some(&bottom)) = (used.first(), used.last()) else { return Vec::new() };
    let limit = top + (bottom - top + 1) * 2 / 3;
    let cell = |x: i64, y: i64| ((0..g).contains(&x) && (0..g).contains(&y)).then(|| rows[y as usize][x as usize]);
    let around = |x: i64, y: i64| [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)];

    let mut seen = std::collections::HashSet::new();
    let mut eyes: Vec<Vec<(i64, i64)>> = Vec::new();
    for y in top as i64..limit as i64 {
        for x in 0..g {
            if cell(x, y) != Some(dark) || seen.contains(&(x, y)) {
                continue;
            }
            let (mut blob, mut stack) = (Vec::new(), vec![(x, y)]);
            while let Some((cx, cy)) = stack.pop() {
                if seen.contains(&(cx, cy)) || cell(cx, cy) != Some(dark) {
                    continue;
                }
                seen.insert((cx, cy));
                blob.push((cx, cy));
                stack.extend(around(cx, cy));
            }
            let span = |f: fn(&(i64, i64)) -> i64| blob.iter().map(f).max().unwrap() - blob.iter().map(f).min().unwrap();
            if blob.len() > 4 || span(|p| p.0) > 1 || span(|p| p.1) > 1 {
                continue;
            }
            let touches_air = blob.iter().any(|&(cx, cy)| around(cx, cy).iter().any(|&(nx, ny)| matches!(cell(nx, ny), None | Some(b'.'))));
            if !touches_air {
                eyes.push(blob);
            }
        }
    }
    if !(1..=2).contains(&eyes.len()) {
        return Vec::new();
    }
    let mut cells: Vec<(usize, usize)> = eyes.into_iter().flatten().map(|(x, y)| (x as usize, y as usize)).collect();
    cells.sort();
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(rows: &[&str]) -> Image {
        let px = rows
            .iter()
            .flat_map(|r| r.bytes())
            .map(|c| match c {
                b'a' => [200, 100, 50, 255],
                b'k' => [10, 10, 10, 255],
                b'h' => [200, 100, 50, 60], // casi transparente
                _ => [0, 0, 0, 0],
            })
            .collect();
        Image { w: rows[0].len(), h: rows.len(), px }
    }

    #[test]
    fn crops_centers_and_finds_eyes() {
        let img = image(&["......", ".aaaa.", ".akka.", ".aaaa.", ".aaaa.", "......"]);
        let (rows, palette) = pixelate(&img, 8, COLORS).unwrap();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[4], "..aaaa..", "centrado y con los pies en la última fila");
        assert_eq!(rows[7], "..aaaa..");
        assert_eq!(palette.len(), 2);
        assert_eq!(find_eyes(&rows, &palette), vec![(3, 5), (4, 5)]);
    }

    #[test]
    fn large_icons_shrink_and_faint_pixels_drop() {
        let img = image(&["aaaah", "aaaah", "aaaah", "aaaah"]);
        let (rows, _) = pixelate(&img, 2, COLORS).unwrap();
        assert_eq!(rows, ["aa", "aa"]);
        assert!(pixelate(&image(&["...", ".h."]), 4, COLORS).is_some_and(|(r, _)| r.iter().all(|r| r == "....")));
        assert!(pixelate(&image(&["...", "..."]), 4, COLORS).is_none());
    }

    #[test]
    fn quantize_keeps_at_most_n_colors() {
        let cells: Vec<Vec<Option<Rgb>>> = vec![(0..40u8).map(|i| Some([i * 6, 255 - i * 6, i])).collect(), vec![None; 40]];
        let out = quantize(cells.clone(), 12);
        let mut seen: Vec<Rgb> = out.iter().flatten().flatten().copied().collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 12);
        assert!(out[1].iter().all(Option::is_none), "lo transparente sigue igual");
        assert_eq!(quantize(cells[..1].to_vec(), 50), cells[..1].to_vec(), "con pocos colores no cambia nada");
    }
}
