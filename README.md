# Claude Pet

Un monito de escritorio por cada sesión activa de [Claude Code](https://claude.com/claude-code). Camina sobre la barra de tareas o el Dock, trepa las paredes, se sube a tus ventanas y celebra cuando Claude termina una tarea. Está escrito en Rust nativo, sin webview, y funciona en Windows y macOS.

## Instalar

### macOS

Opción 1: compilar (recomendado)

```sh
curl https://sh.rustup.rs -sSf | sh     # si no tienes Rust
git clone https://github.com/FrankRSC/claude-pet.git
cd claude-pet
cargo build --release
./target/release/claude-pet install
```

Opción 2: binario ya compilado. Descarga `claude-pet-macos` del [último release](https://github.com/FrankRSC/claude-pet/releases/latest). Es universal (Intel y Apple Silicon). Luego:

```sh
mv claude-pet-macos claude-pet
xattr -d com.apple.quarantine claude-pet   # macOS bloquea binarios descargados sin firmar
chmod +x claude-pet
./claude-pet install
```

Con `install`, el binario se copia a `~/Library/Application Support/claude-pet/`, se registran los hooks en `~/.claude/settings.json` (con respaldo en `settings.json.bak`) y se agrega un LaunchAgent para que arranque al iniciar sesión.

### Windows

Opción 1: binario ya compilado. Descarga `claude-pet-windows.exe` del [último release](https://github.com/FrankRSC/claude-pet/releases/latest) y córrelo con `install`:

```
claude-pet-windows.exe install
```

El binario no está firmado, así que Windows SmartScreen puede mostrar "Windows protegió tu PC": dale a **Más información → Ejecutar de todas formas**.

Opción 2: compilar. Toolchain GNU de Rust más MinGW-w64, que da `dlltool` (también sirve la toolchain MSVC si tienes Visual Studio Build Tools):

```
winget install BrechtSanders.WinLibs.POSIX.UCRT
cargo build --release
target\release\claude-pet.exe install
```

Se instala en `%LOCALAPPDATA%\claude-pet\` y arranca solo al iniciar sesión.

## Comandos

```
claude-pet install [--no-autostart]   # instala hooks (y autoarranque)
claude-pet uninstall                  # quita hooks y autoarranque
claude-pet run --demo                 # abre la app con un monito de prueba
claude-pet size <3-8>                 # tamaño en px por celda (5 = normal)
claude-pet floors                     # diagnóstico: ventanas y plataformas detectadas
claude-pet quit
```

## Qué hace

| Evento de Claude Code | Monito |
|---|---|
| SessionStart | aparece y cae desde arriba |
| UserPromptSubmit | camina rápido con "…" (trabajando) |
| Stop | salta, dice "¡Listo!" y lanza una notificación del sistema |
| Notification | agita los brazos (permiso o atención) |
| SessionEnd, o si el proceso de claude muere | se despide y desaparece |

**Contigo:**
- **Clic:** globo con el proyecto y su estado.
- **Arrastrar y soltar:** lo lanzas. Si choca con otro monito, lo tumba. Si lo sueltas sobre una ventana, aterriza en ella.
- **Clic derecho:** dormir o despertar.
- **⌥ Option + clic** (Alt + clic en Windows) **o clic central:** cambiar su avatar, solo el de ese monito. Se recuerda para su proyecto; elegir un avatar desde el menú borra esas elecciones.
- **Pasar el cursor encima:** se detiene y se pone feliz. A veces te persigue.

**Con la pantalla:**
- **Ventanas:** usan el borde de arriba de cualquier ventana no maximizada como plataforma. Saltan a ella, caminan encima y se tiran por la orilla. Si mueves la ventana, se van con ella. Si la cierras, la minimizas o la tapas con otra, se caen.
- **Bordes:** caminan sobre la barra de tareas o el Dock, trepan las paredes, andan por el techo y pasan de un monitor a otro.

**Bandeja / barra de menú:** monito de prueba, Avatar, Tamaño, ocultar, notificaciones on/off, buscar actualización, salir. La configuración se guarda en `config.json` junto al binario instalado.

## Avatares

Hay 24: Monito, Gato, Perro, Robot, Fantasma, Alien, Panda, Pingüino, Zorro, Conejo, Dinosaurio, Nube, Esqueleto, Bruja, Fuego, Planta, Hielo, Roca, Ninja, Vaquero, Astronauta, Fantasma de fuego, Tiburón y Mago. Por defecto cada sesión recibe uno distinto.

Para agregar uno, añade un `Skin` en `src/skins.rs` con un sprite de 16×16 y su paleta. Una fila de 8 caracteres se refleja y queda simétrica. Las poses (parpadeo, caminar, salto, sentado, dormido) y el contorno se generan solos. `cargo test` revisa los sprites y genera `target/avatars.png` y `target/avatars_big.png` para verlos.

## Actualizaciones

La app se actualiza sola desde los [releases de GitHub](https://github.com/FrankRSC/claude-pet/releases):

- **Automático:** cada 6 horas revisa si hay una versión nueva. Si la hay, te avisa con una notificación (una sola vez por versión) y el menú cambia a **"Actualizar a vX.Y.Z"**.
- **Manual:** en el menú, **"Buscar actualización (vX.Y.Z)"** revisa en ese momento. Si hay una más nueva, la descarga, reemplaza el binario instalado y reinicia la app.

Mensajes posibles:

| Mensaje | Qué significa |
|---|---|
| Ya tienes la última versión | Tu versión es igual o más nueva que el último release. |
| No pude consultar GitHub (¿hay releases publicados?) | Sin internet, GitHub no respondió o el repo todavía no tiene releases. |
| El release vX.Y.Z no trae … | El release existe pero no trae el binario de tu sistema. |
| Falló la descarga | La descarga se cortó o llegó incompleta; el binario instalado no se tocó. |

El actualizador instala el binario del último release sin verificar firmas: confía en quien pueda publicar releases en este repo.

## Publicar una versión

Los releases los publica el CI cuando subes un tag `v*`. Para sacar la versión `0.2.0`:

1. Sube `version = "0.2.0"` en `Cargo.toml` y fusiónalo a `main` (con PR, ver abajo).
2. Crea y sube el tag, **igual** a la versión de `Cargo.toml`:
   ```sh
   git checkout main && git pull
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. El CI (pestaña **Actions**) compila en macOS y Windows, revisa que el tag coincida con `Cargo.toml` (si no, falla sin publicar nada) y crea el release con `claude-pet-macos` y `claude-pet-windows.exe`.

Las apps instaladas lo detectan en las siguientes 6 horas, o al momento con "Buscar actualización".

## Contribuir

`main` está protegida por la regla **"Proteger main"**:

- Todo cambio entra por **pull request**, con el CI verde en macOS y Windows.
- Hace falta **1 aprobación** de alguien distinto al autor del PR. Si hay cambios después de aprobar, se vuelve a pedir la aprobación.
- No se puede borrar `main` ni hacer force-push.

Cualquiera puede proponer cambios con un PR desde un fork, pero solo quien tiene permiso de escritura puede fusionar, y siempre con la aprobación del dueño. El dueño del repo (admin) puede fusionar sus propios PRs saltándose la aprobación: en la página del PR marca **"Merge without waiting for requirements to be met (bypass rules)"**, o usa `gh pr merge <número> --admin`.

## Estructura

```
src/app.rs          event loop (winit), ventanas, bandeja, notificaciones
src/pet.rs          física y máquina de estados
src/geom.rs         áreas de pantalla y plataformas (bordes de ventanas)
src/render.rs       dibujo del sprite y los globos (buffer ARGB)
src/sprites.rs      poses y contorno generados a partir de los sprites
src/skins.rs        los 23 avatares extra
src/hook_client.rs  `claude-pet hook`, lo que ejecuta Claude Code
src/installer.rs    install/uninstall de hooks
src/platform/       Windows (layered windows, Win32) y macOS (AppKit/CoreGraphics)
```

Con `CLAUDE_PET_DEBUG=1`, la app escribe un log en la carpeta temporal (`claude-pet.log`).
