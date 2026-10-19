# Claude Pet

Un monito de escritorio por cada sesión activa de [Claude Code](https://claude.com/claude-code) (y también de Codex, Gemini CLI y OpenCode; ver [Agentes compatibles](#agentes-compatibles)). Camina sobre la barra de tareas o el Dock, trepa las paredes, se sube a tus ventanas y celebra cuando Claude termina una tarea. Está escrito en Rust nativo, sin webview, y funciona en Windows y macOS.

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

Con `install`, el binario se copia a `~/Library/Application Support/claude-pet/`, se registran los hooks en cada agente que encuentre instalado (Claude Code, Codex, Gemini CLI, OpenCode; ver [Agentes compatibles](#agentes-compatibles), con respaldo `.bak` de cada archivo que toca) y se agrega un LaunchAgent para que arranque al iniciar sesión.

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
claude-pet install [--no-autostart]   # instala hooks en cada agente (y autoarranque)
claude-pet install --agent codex      # solo en uno: claude, codex, gemini u opencode
claude-pet uninstall [--agent …]      # quita hooks y autoarranque
claude-pet run --demo                 # abre la app con un monito de prueba
claude-pet size <2-5>                 # tamaño en px por celda (3 = normal)
claude-pet skins <skins.json>         # carga avatares (ver Avatares)
claude-pet floors                     # diagnóstico: ventanas y plataformas detectadas
claude-pet quit
```

## Qué hace

| Evento (con los nombres de Claude Code) | Monito |
|---|---|
| SessionStart | aparece y cae desde arriba |
| UserPromptSubmit | camina rápido con "…" (trabajando) |
| Stop | salta, dice "¡Listo!" y lanza una notificación del sistema |
| Notification | agita los brazos (permiso o atención) |
| SessionEnd, o si el proceso del agente muere | se despide y desaparece |

**Contigo:**
- **Clic:** globo con el proyecto y su estado.
- **Arrastrar y soltar:** lo lanzas. Si choca con otro monito, lo tumba. Si lo sueltas sobre una ventana, aterriza en ella.
- **Clic derecho:** dormir o despertar.
- **⌥ Option + clic** (Alt + clic en Windows) **o clic central:** cambiar su avatar, solo el de ese monito. Se recuerda para su proyecto; elegir un avatar desde el menú borra esas elecciones.
- **Pasar el cursor encima:** se detiene y se pone feliz. A veces te persigue.

**Con la pantalla:**
- **Ventanas:** usan el borde de arriba de cualquier ventana no maximizada como plataforma. Saltan a ella, caminan encima y se tiran por la orilla. Si mueves la ventana, se van con ella. Si la cierras, la minimizas o la tapas con otra, se caen.
- **Bordes:** caminan sobre la barra de tareas o el Dock, trepan las paredes, andan por el techo y pasan de un monitor a otro.

**Bandeja / barra de menú:** monito de prueba, Avatar, Grupo del aleatorio, Ícono de la barra, Tamaño, ocultar, notificaciones on/off, buscar actualización, salir. La configuración se guarda en `config.json` junto al binario instalado.

- **Avatar:** uno fijo para todos los monitos, o "Aleatorio" (uno distinto por sesión).
- **Grupo del aleatorio:** qué avatares entran en el sorteo (y por los que pasa ⌥+clic). Cada uno se quita o se añade con su palomita; "Toda la generación N" mete o saca la generación completa y "Todos" vuelve a incluirlos todos. Si quitas todos, se sortea entre todos.
- **Ícono de la barra:** qué avatar se dibuja en la barra de menú o la bandeja.

## Agentes compatibles

La app no sabe de ningún agente en particular: escucha cinco eventos por TCP local (`SessionStart`, `UserPromptSubmit`, `Stop`, `Notification`, `SessionEnd`) y `claude-pet hook --agent <nombre>` traduce lo que manda cada CLI. `claude-pet install` registra los hooks en todos los que encuentre (por su carpeta de configuración); con `--agent` se limita a uno.

| Agente | Dónde se instala | Eventos que se traducen |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | los cinco, tal cual |
| Codex CLI | `~/.codex/hooks.json` | `PermissionRequest` → Notification |
| Gemini CLI | `~/.gemini/settings.json` | `BeforeAgent` → UserPromptSubmit, `AfterAgent` → Stop |
| OpenCode | plugin en `~/.config/opencode/plugins/claude-pet.ts` | `session.created`, `chat.message`, `session.idle`, `permission.asked`, `session.deleted` |

- **Codex** no ejecuta hooks que no hayas aprobado: tras instalar, abre Codex y ejecuta `/hooks` para aprobar el de claude-pet. Si lo saltas no habrá monito ni ningún error visible. Si el comando cambia (por ejemplo, al reinstalar en otra ruta) hay que aprobarlo de nuevo.
- **OpenCode** no tiene hooks de shell, así que va un plugin que recibe los eventos en proceso y ejecuta el binario. Ignora las sesiones de subagentes y el "idle" inicial de cada sesión (no es un "¡Listo!"), y manda el PID de OpenCode para que el monito se vaya cuando lo cierres.
- **Otros CLI con hooks** (Cursor, Copilot, Kiro…): configura a mano un hook de tipo comando que ejecute `"/ruta/claude-pet" hook --agent <nombre>` con el JSON del evento por stdin (`hook_event_name`, `session_id`, `cwd`). Los nombres de evento que no se reconocen pasan tal cual, así que sirve cualquiera que coincida con los cinco de arriba.
- El monito muestra el agente en el globo al hacer clic y en los avisos ("Codex terminó"). Para ver qué llega, `CLAUDE_PET_DEBUG=1` escribe cada evento en `claude-pet.log`.

## Avatares

De fábrica solo viene el Monito. Los demás se cargan de un archivo `skins.json` en la carpeta de datos (`~/Library/Application Support/claude-pet/` en macOS, `%LOCALAPPDATA%\claude-pet\` en Windows), junto a `config.json`. Para instalar uno:

```sh
claude-pet skins mi-archivo.json
```

Valida el archivo, lo copia ahí y, si la app está corriendo, recarga los menús sin reiniciar. Con más de 30 avatares los menús se agrupan por generación (según el `id` de cada uno). Por defecto cada sesión recibe un avatar distinto del grupo del aleatorio; las elecciones (avatar fijo, grupo, ícono, avatar por proyecto) se guardan por nombre en `config.json`, así que sobreviven a un `skins.json` nuevo.

El archivo lo genera `tools/pokedex.py` a partir de la Pokédex (ver abajo), pero también se puede escribir a mano:

```json
{
  "grid": 24,
  "skins": [
    {
      "id": 25,
      "name": "Pikachu",
      "rows": ["........................", "…23 filas más…"],
      "palette": { "a": [250, 210, 40], "k": [34, 24, 30] },
      "eyes": [[9, 7], [9, 8], [14, 7], [14, 8]]
    }
  ]
}
```

- `rows`: `grid` filas de `grid` caracteres; `.` es transparente y los pies van en la última fila. Una fila con la mitad de caracteres se refleja (sprite simétrico). Si `grid` es menor que 24, el sprite se centra y se alinea al suelo.
- `palette`: un color `[r, g, b]` por carácter.
- `eyes` (opcional): celdas `[x, y]` que se cierran al parpadear y al dormir. También sirve `"eye": "k"` para marcar todas las celdas de un carácter.
- `id` (opcional): número de Pokédex, solo para agrupar el menú.

Las poses (parpadeo, caminar, salto, sentado, dormido) y el contorno se generan solos. Un avatar mal formado se omite (con `CLAUDE_PET_DEBUG=1` queda en el log) y no tumba la app. `cargo test` valida el `skins.json` instalado y genera `target/avatars.png` y `target/avatars_big.png` para verlos.

### Pokédex

`tools/pokedex.py` convierte los iconos de la Pokédex de [PokeAPI](https://github.com/PokeAPI/sprites) en avatares. Necesita Python 3 y Pillow:

```sh
pip install pillow                                   # o: uv pip install pillow
python3 tools/pokedex.py --gen 1 --preview kanto.png  # los 151 de Kanto
python3 tools/pokedex.py --ids 25,133,143             # solo algunos (también rangos: 1-9)
claude-pet skins skins.json
```

Por cada Pokémon descarga el icono de Generación VII (pixel art de 40×30), lo recorta, lo reduce solo si no cabe en 24×24 (nunca se amplía), deja a lo más 12 colores (`--colors`), lo centra con los pies en la última fila y busca los ojos (manchas chicas del color más oscuro en la parte de arriba) para que parpadee; `--no-eyes` lo desactiva. Los nombres salen en español de la API (`--no-names` los evita). Las descargas se guardan en `tools/.cache/`. Con `--preview` escribe una hoja PNG para revisar el resultado antes de instalarlo.

Los sprites son propiedad de Nintendo, Game Freak y Creatures; PokeAPI los distribuye para uso personal. Por eso el repo no incluye ningún `skins.json`: cada quien lo genera (y `.gitignore` ya excluye `skins.json`, los previews y la caché).

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
src/sprites.rs      el Monito, y las poses y el contorno de cada avatar
src/skins.rs        carga y valida skins.json (avatares en tiempo de ejecución)
src/hook_client.rs  `claude-pet hook --agent …`: traduce el evento del agente y lo manda a la app
src/installer.rs    install/uninstall de hooks por agente
assets/opencode/    plugin de OpenCode (se copia con la ruta del binario)
src/platform/       Windows (layered windows, Win32) y macOS (AppKit/CoreGraphics)
```

Con `CLAUDE_PET_DEBUG=1`, la app escribe un log en la carpeta temporal (`claude-pet.log`).
