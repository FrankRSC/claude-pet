//! Avatares agrupados en estilos. El Monito dibujado a mano vive en
//! `sprites.rs` y siempre es el avatar 0; después van los del estilo "Base"
//! (dibujados a mano, dentro del binario: `assets/base.json`) y luego los de
//! cada archivo de `estilos/` en la carpeta de datos, uno por estilo y en orden
//! de nombre de archivo (`pokemon.json` lo baja `install`). Las poses y el
//! contorno se generan en `sprites::build`.
//!
//! Formato de cada archivo:
//!
//! ```json
//! {
//!   "style": "Pokémon",
//!   "grid": 24,
//!   "skins": [
//!     {
//!       "id": 25,
//!       "name": "Pikachu",
//!       "rows": ["........................", "..."],
//!       "palette": { "a": [250, 210, 40], "b": [34, 24, 30] },
//!       "eyes": [[9, 7], [9, 8], [14, 7], [14, 8]]
//!     }
//!   ]
//! }
//! ```
//!
//! `style` es el nombre del estilo en el menú (sin él, el nombre del archivo).
//! `grid` es el lado de la rejilla de las filas (si es menor que `GRID`, se
//! centra y se alinea al suelo). Una fila con la mitad de caracteres se refleja.
//! `eyes` son celdas (x, y) que se cierran al parpadear o dormir; también se
//! acepta `"eye": "b"` para marcar como ojo todas las celdas de un carácter.
//! `"facing": "left"` indica que el dibujo mira a la izquierda (los iconos de
//! la Pokédex): se voltea al cargar, porque la app asume que todo mira a la
//! derecha.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::Value;

use crate::sprites::{Color, GRID};

pub struct Skin {
    /// Número de Pokédex (o el que ponga el archivo); agrupa el menú por generación.
    pub id: Option<u32>,
    pub name: String,
    /// Estilo al que pertenece ("Base", "Pokémon"…): agrupa el menú y el clic derecho.
    pub style: String,
    /// `GRID` filas de `GRID` bytes; `.` es transparente.
    pub rows: Vec<Vec<u8>>,
    pub palette: Vec<(u8, Color)>,
    /// Celdas (x, y) de los ojos.
    pub eyes: Vec<(usize, usize)>,
}

static SKINS: RwLock<Vec<Skin>> = RwLock::new(Vec::new());

/// El estilo que viene en el binario, con el Monito.
pub const BASE: &str = "Base";
const BASE_FILE: &str = include_str!("../assets/base.json");
/// El estilo que baja `install` (`pokedex.rs`).
pub const POKEMON: &str = "Pokémon";

/// Rangos de la Pokédex nacional por generación.
const GENERATIONS: [(u32, u32); 9] = [
    (1, 151),
    (152, 251),
    (252, 386),
    (387, 493),
    (494, 649),
    (650, 721),
    (722, 809),
    (810, 905),
    (906, 1025),
];

pub fn generation(id: u32) -> Option<u32> {
    GENERATIONS.iter().position(|(a, b)| (*a..=*b).contains(&id)).map(|i| i as u32 + 1)
}

/// Carpeta de datos de la app (config.json, estilos/, cries/).
pub fn dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("claude-pet"))
}

/// Un archivo por estilo, junto a `config.json`.
pub fn styles_dir() -> Option<PathBuf> {
    Some(dir()?.join("estilos"))
}

/// Archivo del estilo: `estilos/pokemon.json` para "Pokémon".
pub fn style_path(style: &str) -> Option<PathBuf> {
    Some(styles_dir()?.join(format!("{}.json", slug(style))))
}

fn slug(style: &str) -> String {
    let plain: String = style
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    let s = plain.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "estilo".into() } else { s }
}

/// Ya están los Pokémon (bajados por `install` o por `claude-pet pokemon`).
pub fn has_pokemon() -> bool {
    migrate();
    style_path(POKEMON).is_some_and(|p| p.exists())
}

/// Hasta la 0.4 había un solo `skins.json` sin estilo; pasa a `estilos/`.
fn migrate() {
    // Los tests leen la carpeta de datos real; no la cambian.
    if cfg!(test) {
        return;
    }
    let Some(old) = dir().map(|d| d.join("skins.json")).filter(|p| p.exists()) else { return };
    let Some(mut v) = std::fs::read_to_string(&old).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return };
    if !v.is_object() {
        return;
    }
    let style = default_style(&v);
    v["style"] = style.clone().into();
    let Some(new) = style_path(&style) else { return };
    let _ = std::fs::create_dir_all(new.parent().unwrap());
    if new.exists() || std::fs::write(&new, v.to_string()).is_ok() {
        let _ = std::fs::remove_file(&old);
    }
}

/// El estilo de un archivo que no lo dice: los de la Pokédex traen `id`.
fn default_style(v: &Value) -> String {
    if let Some(s) = v["style"].as_str().filter(|s| !s.trim().is_empty()) {
        return s.trim().to_string();
    }
    let pokedex = v["skins"].as_array().is_some_and(|l| l.iter().any(|s| s["id"].is_u64()));
    if pokedex { POKEMON.into() } else { "Personalizados".into() }
}

/// Lo que haya cargado, bajo un candado de lectura.
pub fn with<R>(f: impl FnOnce(&[Skin]) -> R) -> R {
    f(&SKINS.read().unwrap())
}

/// Vuelve a leer el estilo Base y los archivos de `estilos/` y reemplaza la
/// lista. Devuelve cuántos avatares quedaron; los inválidos se saltan con una
/// línea en el log.
pub fn reload() -> usize {
    migrate();
    let mut files: Vec<PathBuf> = styles_dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut loaded = Vec::new();
    let mut add = |what: &str, text: Result<String, String>| match text.and_then(|t| parse(&t)) {
        Ok((skins, errors)) => {
            for e in errors {
                crate::app::debug_log(format_args!("{what}: {e}"));
            }
            loaded.extend(skins);
        }
        Err(e) => crate::app::debug_log(format_args!("{what}: {e}")),
    };
    add("base.json", Ok(BASE_FILE.to_string()));
    for f in &files {
        add(&f.display().to_string(), std::fs::read_to_string(f).map_err(|e| e.to_string()));
    }
    let n = loaded.len();
    *SKINS.write().unwrap() = loaded;
    n
}

/// Parsea y valida un archivo de estilo. Err si el archivo entero no sirve; si
/// no, los avatares válidos y un mensaje por cada uno que se descartó.
pub fn parse(text: &str) -> Result<(Vec<Skin>, Vec<String>), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("no es JSON válido: {e}"))?;
    let style = default_style(&v);
    let grid = match v.get("grid") {
        None => GRID,
        Some(g) => g.as_u64().map(|g| g as usize).filter(|g| (1..=GRID).contains(g)).ok_or_else(|| {
            format!("\"grid\" debe ser un entero entre 1 y {GRID} (esta versión dibuja a {GRID}×{GRID})")
        })?,
    };
    let list = v["skins"].as_array().ok_or("falta la lista \"skins\"")?;
    let mut skins = Vec::new();
    let mut errors = Vec::new();
    for (i, item) in list.iter().enumerate() {
        match parse_skin(item, grid, &style) {
            Ok(s) => skins.push(s),
            Err(e) => {
                let who = item["name"].as_str().map_or_else(|| format!("#{i}"), str::to_string);
                errors.push(format!("{who}: {e}"));
            }
        }
    }
    Ok((skins, errors))
}

fn parse_skin(v: &Value, grid: usize, style: &str) -> Result<Skin, String> {
    let name = v["name"].as_str().filter(|n| !n.trim().is_empty()).ok_or("sin \"name\"")?.to_string();
    let id = v["id"].as_u64().map(|n| n as u32);

    let mut palette: Vec<(u8, Color)> = Vec::new();
    let entries = v["palette"].as_object().ok_or("\"palette\" debe ser un objeto { carácter: [r, g, b] }")?;
    for (key, rgb) in entries {
        let &[k] = key.as_bytes() else { return Err(format!("clave de paleta '{key}': debe ser un solo carácter ASCII")) };
        if k == b'.' {
            return Err("'.' es el transparente; no puede estar en la paleta".into());
        }
        let c: Vec<u8> = rgb
            .as_array()
            .filter(|a| a.len() == 3)
            .and_then(|a| a.iter().map(|x| x.as_u64().filter(|&n| n < 256).map(|n| n as u8)).collect())
            .ok_or_else(|| format!("color de '{key}': debe ser [r, g, b]"))?;
        palette.push((k, [c[0], c[1], c[2]]));
    }
    if palette.is_empty() {
        return Err("paleta vacía".into());
    }

    let src = v["rows"].as_array().ok_or("\"rows\" debe ser una lista de cadenas")?;
    if src.len() != grid {
        return Err(format!("{} filas; deben ser {grid}", src.len()));
    }
    let (ox, oy) = ((GRID - grid) / 2, GRID - grid);
    let mut rows = vec![vec![b'.'; GRID]; GRID];
    for (y, row) in src.iter().enumerate() {
        let row = row.as_str().ok_or_else(|| format!("fila {y}: no es una cadena"))?.as_bytes();
        let full: Vec<u8> = if row.len() == grid {
            row.to_vec()
        } else if grid % 2 == 0 && row.len() == grid / 2 {
            row.iter().chain(row.iter().rev()).copied().collect()
        } else {
            return Err(format!("fila {y}: {} caracteres; deben ser {grid}", row.len()));
        };
        for (x, &c) in full.iter().enumerate() {
            if c != b'.' && !palette.iter().any(|(k, _)| *k == c) {
                return Err(format!("fila {y}: '{}' no está en la paleta", c as char));
            }
            rows[oy + y][ox + x] = c;
        }
    }

    let mut eyes = Vec::new();
    if let Some(list) = v["eyes"].as_array() {
        for cell in list {
            let xy: Vec<usize> = cell
                .as_array()
                .filter(|a| a.len() == 2)
                .and_then(|a| a.iter().map(|n| n.as_u64().map(|n| n as usize)).collect())
                .filter(|xy: &Vec<usize>| xy[0] < grid && xy[1] < grid)
                .ok_or_else(|| format!("ojo {cell}: debe ser [x, y] dentro de la rejilla"))?;
            eyes.push((ox + xy[0], oy + xy[1]));
        }
    }
    // La app dibuja todo mirando a la derecha y lo voltea al caminar a la
    // izquierda; los iconos de la Pokédex miran a la izquierda.
    match v["facing"].as_str() {
        None | Some("right") => {}
        Some("left") => {
            for row in &mut rows {
                row.reverse();
            }
            for (x, _) in &mut eyes {
                *x = GRID - 1 - *x;
            }
        }
        Some(other) => return Err(format!("\"facing\" '{other}': debe ser \"left\" o \"right\"")),
    }
    if let Some(ch) = v["eye"].as_str() {
        let &[k] = ch.as_bytes() else { return Err(format!("\"eye\" '{ch}': debe ser un solo carácter")) };
        for (y, row) in rows.iter().enumerate() {
            for (x, &c) in row.iter().enumerate() {
                if c == k && !eyes.contains(&(x, y)) {
                    eyes.push((x, y));
                }
            }
        }
    }
    Ok(Skin { id, name, style: style.to_string(), rows, palette, eyes })
}

/// `claude-pet skins <archivo>`: valida, lo guarda como el archivo de su
/// estilo (reemplaza el que hubiera de ese estilo) y pide a la app que recargue.
pub fn install(src: &Path) {
    let text = match std::fs::read_to_string(src) {
        Ok(t) => t,
        Err(e) => return eprintln!("No pude leer {}: {e}", src.display()),
    };
    let (skins, errors) = match parse(&text) {
        Ok(r) => r,
        Err(e) => return eprintln!("{}: {e}", src.display()),
    };
    for e in &errors {
        eprintln!("  se omite {e}");
    }
    let Some(style) = skins.first().map(|s| s.style.clone()) else { return eprintln!("{}: sin avatares", src.display()) };
    if let Err(e) = write_style(&style, &text) {
        return eprintln!("{e}");
    }
    println!("{} avatares en el estilo {style}", skins.len());
    // Los gritos que bajó tools/pokedex.py viven en cries/ junto al JSON.
    let cries = src.parent().unwrap_or(Path::new(".")).join("cries");
    if let (Ok(entries), Some(to)) = (std::fs::read_dir(&cries), dir().map(|d| d.join("cries"))) {
        let _ = std::fs::create_dir_all(&to);
        let copied = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "ogg"))
            .filter(|e| std::fs::copy(e.path(), to.join(e.file_name())).is_ok())
            .count();
        println!("{copied} gritos en {}", to.display());
    }
    let msg = serde_json::json!({ "cmd": "reload" });
    if crate::ipc::send(&msg, std::time::Duration::from_millis(500)) {
        println!("Claude Pet ya los tiene; están en el menú Avatar → {style}.");
    } else {
        println!("Claude Pet no está corriendo; los cargará al arrancar.");
    }
}

/// Guarda `text` como el archivo del estilo, con el `style` puesto.
pub fn write_style(style: &str, text: &str) -> Result<(), String> {
    let path = style_path(style).ok_or("No encuentro la carpeta de datos")?;
    let mut v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    v["style"] = style.into();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    std::fs::write(&path, v.to_string() + "\n").map_err(|e| format!("No pude escribir {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "grid": 4,
        "skins": [
            { "id": 25, "name": "Par", "rows": ["..", ".a", "ab", "bb"], "palette": {"a": [1, 2, 3], "b": [4, 5, 6]}, "eyes": [[1, 1]] },
            { "name": "Ojo por carácter", "rows": ["....", ".kk.", ".aa.", "aaaa"], "palette": {"a": [1, 2, 3], "k": [0, 0, 0]}, "eye": "k" },
            { "name": "Fila corta", "rows": ["...", "....", "....", "...."], "palette": {"a": [1, 2, 3]} },
            { "name": "Sin paleta", "rows": ["....", "....", "....", "..x."], "palette": {"a": [1, 2, 3]} },
            { "rows": ["....", "....", "....", "...."], "palette": {"a": [1, 2, 3]} }
        ]
    }"#;

    #[test]
    fn pads_mirrors_and_validates() {
        let (skins, errors) = parse(SAMPLE).unwrap();
        assert_eq!(skins.len(), 2);
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].starts_with("Fila corta:"));
        assert!(errors[1].contains("'x' no está en la paleta"));
        assert!(errors[2].starts_with("#4:"));

        // 4×4 dentro de GRID: centrado y con los pies en la última fila.
        let s = &skins[0];
        let (ox, oy) = ((GRID - 4) / 2, GRID - 4);
        assert_eq!(s.id, Some(25));
        assert_eq!(s.rows.len(), GRID);
        assert!(s.rows.iter().all(|r| r.len() == GRID));
        assert_eq!(&s.rows[oy + 1][ox..ox + 4], b".aa.", "la fila de 2 se refleja");
        assert_eq!(&s.rows[oy + 3][ox..ox + 4], b"bbbb");
        assert_eq!(s.eyes, vec![(ox + 1, oy + 1)]);
        assert_eq!(skins[1].eyes, vec![(ox + 1, oy + 1), (ox + 2, oy + 1)]);
    }

    #[test]
    fn left_facing_sprites_are_mirrored() {
        let file = r#"{"grid": 4, "skins": [
            {"name": "Izq", "facing": "left", "rows": ["....", "ak..", "aa..", "aaa."], "palette": {"a": [1, 2, 3], "k": [0, 0, 0]}, "eyes": [[1, 1]]},
            {"name": "Mal", "facing": "up", "rows": ["....", "....", "....", "a..."], "palette": {"a": [1, 2, 3]}}
        ]}"#;
        let (skins, errors) = parse(file).unwrap();
        assert_eq!(errors.len(), 1);
        let (ox, oy) = ((GRID - 4) / 2, GRID - 4);
        let s = &skins[0];
        assert_eq!(&s.rows[oy + 1][ox..ox + 4], b"..ka");
        assert_eq!(s.eyes, vec![(GRID - 1 - (ox + 1), oy + 1)]);
        assert_eq!(s.rows[oy + 1][GRID - 1 - (ox + 1)], b'k', "el ojo sigue sobre su celda");
    }

    #[test]
    fn rejects_bad_files() {
        assert!(parse("[]").is_err());
        assert!(parse("{\"grid\": 99, \"skins\": []}").is_err());
        assert!(parse("no json").is_err());
        assert_eq!(parse("{\"skins\": []}").unwrap().0.len(), 0);
    }

    #[test]
    fn generations() {
        assert_eq!(generation(1), Some(1));
        assert_eq!(generation(151), Some(1));
        assert_eq!(generation(152), Some(2));
        assert_eq!(generation(1025), Some(9));
        assert_eq!(generation(0), None);
        assert_eq!(generation(5000), None);
    }

    #[test]
    fn base_style_loads_whole() {
        let (skins, errors) = parse(BASE_FILE).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(skins.len(), 23);
        assert!(skins.iter().all(|s| s.style == BASE && !s.eyes.is_empty()));
    }

    #[test]
    fn style_comes_from_the_file_or_its_ids() {
        let (s, _) = parse(r#"{"style": "Robots", "grid": 1, "skins": [{"name": "R", "rows": ["a"], "palette": {"a": [1, 2, 3]}}]}"#).unwrap();
        assert_eq!(s[0].style, "Robots");
        let (s, _) = parse(r#"{"grid": 1, "skins": [{"id": 1, "name": "B", "rows": ["a"], "palette": {"a": [1, 2, 3]}}]}"#).unwrap();
        assert_eq!(s[0].style, POKEMON, "un skins.json viejo de la Pokédex");
        let (s, _) = parse(r#"{"grid": 1, "skins": [{"name": "Yo", "rows": ["a"], "palette": {"a": [1, 2, 3]}}]}"#).unwrap();
        assert_eq!(s[0].style, "Personalizados");
        assert_eq!(slug(POKEMON), "pokemon");
        assert_eq!(slug("Mis Ñoños 2!"), "mis-nonos-2");
        assert_eq!(slug("??"), "estilo");
    }

    /// Los estilos instalados (si hay) deben cargar sin descartar nada.
    #[test]
    fn installed_files_are_well_formed() {
        let files = styles_dir().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten().map(|e| e.path());
        for p in files.filter(|p| p.extension().is_some_and(|x| x == "json")) {
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let (skins, errors) = parse(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(errors.is_empty(), "{}: {errors:?}", p.display());
            assert!(!skins.is_empty(), "{}: sin avatares", p.display());
        }
    }
}
