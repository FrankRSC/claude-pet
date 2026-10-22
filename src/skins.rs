//! Avatares cargados en tiempo de ejecución desde `skins.json` (lo genera
//! `tools/pokedex.py`, o se escribe a mano). El Monito dibujado a mano vive en
//! `sprites.rs` y siempre es el avatar 0; estos van después, en el orden del
//! archivo. Las poses y el contorno se generan en `sprites::build`.
//!
//! Formato del archivo:
//!
//! ```json
//! {
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
    /// `GRID` filas de `GRID` bytes; `.` es transparente.
    pub rows: Vec<Vec<u8>>,
    pub palette: Vec<(u8, Color)>,
    /// Celdas (x, y) de los ojos.
    pub eyes: Vec<(usize, usize)>,
}

static SKINS: RwLock<Vec<Skin>> = RwLock::new(Vec::new());

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

/// Carpeta de datos de la app (config.json, skins.json, cries/).
pub fn dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("claude-pet"))
}

/// Donde la app busca los avatares: junto a `config.json`.
pub fn path() -> Option<PathBuf> {
    Some(dir()?.join("skins.json"))
}

/// Lo que haya cargado, bajo un candado de lectura.
pub fn with<R>(f: impl FnOnce(&[Skin]) -> R) -> R {
    f(&SKINS.read().unwrap())
}

/// Vuelve a leer `skins.json` (si existe) y reemplaza la lista. Devuelve
/// cuántos avatares quedaron; los inválidos se saltan con una línea en el log.
pub fn reload() -> usize {
    let loaded = match path().filter(|p| p.exists()) {
        Some(p) => match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| parse(&t)) {
            Ok((skins, errors)) => {
                for e in errors {
                    crate::app::debug_log(format_args!("skins.json: {e}"));
                }
                skins
            }
            Err(e) => {
                crate::app::debug_log(format_args!("skins.json: {e}"));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let n = loaded.len();
    *SKINS.write().unwrap() = loaded;
    n
}

/// Parsea y valida el archivo. Err si el archivo entero no sirve; si no,
/// los avatares válidos y un mensaje por cada uno que se descartó.
pub fn parse(text: &str) -> Result<(Vec<Skin>, Vec<String>), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("no es JSON válido: {e}"))?;
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
        match parse_skin(item, grid) {
            Ok(s) => skins.push(s),
            Err(e) => {
                let who = item["name"].as_str().map_or_else(|| format!("#{i}"), str::to_string);
                errors.push(format!("{who}: {e}"));
            }
        }
    }
    Ok((skins, errors))
}

fn parse_skin(v: &Value, grid: usize) -> Result<Skin, String> {
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
    Ok(Skin { id, name, rows, palette, eyes })
}

/// `claude-pet skins <archivo>`: valida, copia a la carpeta de datos y pide a
/// la app que recargue.
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
    let Some(dst) = path() else { return eprintln!("No encuentro la carpeta de datos") };
    if let Some(dir) = dst.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&dst, &text) {
        return eprintln!("No pude escribir {}: {e}", dst.display());
    }
    println!("{} avatares en {}", skins.len(), dst.display());
    // Los gritos que bajó tools/pokedex.py viven en cries/ junto al JSON.
    let cries = src.parent().unwrap_or(Path::new(".")).join("cries");
    if let (Ok(entries), Some(to)) = (std::fs::read_dir(&cries), dst.parent().map(|d| d.join("cries"))) {
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
        println!("Claude Pet ya los tiene; están en el menú Avatar.");
    } else {
        println!("Claude Pet no está corriendo; los cargará al arrancar.");
    }
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

    /// El skins.json instalado (si hay) debe cargar sin descartar nada.
    #[test]
    fn installed_file_is_well_formed() {
        for p in [path(), Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/skins.json")))].into_iter().flatten() {
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let (skins, errors) = parse(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(errors.is_empty(), "{}: {errors:?}", p.display());
            assert!(!skins.is_empty(), "{}: sin avatares", p.display());
        }
    }
}
