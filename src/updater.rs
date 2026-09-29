//! "Buscar actualización": compara la versión con el último release de GitHub y,
//! si hay una más nueva, descarga el binario y lo pone en lugar del actual.
//! Usa el `curl` del sistema (viene en macOS y en Windows 10+) para no cargar
//! un cliente HTTP con TLS dentro del binario.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

const LATEST: &str = "https://api.github.com/repos/FrankRSC/claude-pet/releases/latest";
const ASSET: &str = if cfg!(windows) { "claude-pet-windows.exe" } else { "claude-pet-macos" };

pub enum Outcome {
    UpToDate,
    /// Ya quedó instalada en esta ruta; hay que reiniciar la app desde ahí.
    Installed { version: String, exe: PathBuf },
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn curl() -> Command {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--max-time", "120", "-H", "User-Agent: claude-pet"])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn parse(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

fn latest_release() -> Result<Value, String> {
    // Para probar contra un release falso sin publicar nada.
    let url = std::env::var("CLAUDE_PET_RELEASES_URL").unwrap_or_else(|_| LATEST.into());
    let out = curl().arg(url).output().map_err(|_| "No encontré curl".to_string())?;
    if !out.status.success() {
        return Err("No pude consultar GitHub (¿hay releases publicados?)".into());
    }
    serde_json::from_slice(&out.stdout).map_err(|_| "Respuesta rara de GitHub".into())
}

/// La versión publicada si es más nueva que esta. Sin red o sin releases,
/// None: es la revisión automática y no vale la pena avisar de errores.
pub fn available() -> Option<String> {
    let release = latest_release().ok()?;
    let tag = release["tag_name"].as_str()?;
    newer(tag, current()).then(|| tag.trim_start_matches('v').to_string())
}

/// Busca y, si hay versión nueva, la instala. Tarda: llamarla fuera del hilo
/// principal.
pub fn check_and_install() -> Result<Outcome, String> {
    let release = latest_release()?;
    let tag = release["tag_name"].as_str().ok_or("El release no tiene versión")?;
    if !newer(tag, current()) {
        return Ok(Outcome::UpToDate);
    }
    let url = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == ASSET))
        .and_then(|x| x["browser_download_url"].as_str())
        .ok_or_else(|| format!("El release {tag} no trae {ASSET}"))?;
    let exe = std::env::current_exe().map_err(|_| "No encuentro mi ejecutable")?;
    replace(&exe, url)?;
    Ok(Outcome::Installed { version: tag.trim_start_matches('v').to_string(), exe })
}

fn replace(exe: &Path, url: &str) -> Result<(), String> {
    let new = exe.with_extension("new");
    let ok = curl().arg("-o").arg(&new).arg(url).status().is_ok_and(|s| s.success());
    // Un binario de verdad pesa megas; menos que eso es una página de error.
    if !ok || std::fs::metadata(&new).map_or(0, |m| m.len()) < 500_000 {
        let _ = std::fs::remove_file(&new);
        return Err("Falló la descarga".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    // Windows no deja sobrescribir un .exe en uso, pero sí renombrarlo.
    #[cfg(windows)]
    {
        let old = old_path(exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).map_err(|_| "No pude mover el ejecutable actual")?;
    }
    std::fs::rename(&new, exe).map_err(|_| "No pude poner la versión nueva".to_string())
}

fn old_path(exe: &Path) -> PathBuf {
    exe.with_extension("old")
}

/// Borra el ejecutable que dejó la actualización anterior en Windows.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn compara_versiones() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("v0.10.0", "0.9.3"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("v0.1.0", "0.2.0"));
    }
}

#[cfg(all(test, unix))]
mod replace_tests {
    #[test]
    fn reemplaza_el_binario() {
        let dir = std::env::temp_dir().join(format!("claude-pet-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("claude-pet");
        let nuevo = dir.join("descarga");
        std::fs::write(&exe, b"viejo").unwrap();
        std::fs::write(&nuevo, vec![7u8; 600_000]).unwrap();
        super::replace(&exe, &format!("file://{}", nuevo.display())).unwrap();
        assert_eq!(std::fs::metadata(&exe).unwrap().len(), 600_000);
        assert!(!exe.with_extension("new").exists());
        // Una descarga chica (página de error) no toca el binario.
        std::fs::write(&nuevo, b"404").unwrap();
        assert!(super::replace(&exe, &format!("file://{}", nuevo.display())).is_err());
        assert_eq!(std::fs::metadata(&exe).unwrap().len(), 600_000);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
