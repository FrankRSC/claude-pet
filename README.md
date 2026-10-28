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

Con `install`, el binario se copia a `~/Library/Application Support/claude-pet/`, se registran los hooks en cada agente que encuentre instalado (Claude Code, Codex, Gemini CLI, OpenCode; ver [Agentes compatibles](#agentes-compatibles), con respaldo `.bak` de cada archivo que toca), se bajan los Pokémon (ver [Avatares](#avatares)) y se agrega un LaunchAgent para que arranque al iniciar sesión.

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

Se instala en `%LOCALAPPDATA%\claude-pet\`, baja los Pokémon y arranca solo al iniciar sesión.

## Comandos

```
claude-pet install [--no-autostart]   # instala hooks en cada agente (y autoarranque)
claude-pet install --no-pokemon       # sin bajar los Pokémon (solo el Monito)
claude-pet install --agent codex      # solo en uno: claude, codex, gemini u opencode
claude-pet uninstall [--agent …]      # quita hooks y autoarranque
claude-pet run --demo                 # abre la app con un monito de prueba
claude-pet size <2-5>                 # tamaño en px por celda (3 = normal)
claude-pet skins <archivo.json>       # agrega o reemplaza un estilo de avatares (ver Avatares)
claude-pet pokemon                    # vuelve a bajar los Pokémon (reemplaza ese estilo)
claude-pet floors                     # diagnóstico: ventanas y plataformas detectadas
claude-pet restart                    # reinicia la app y vuelve a buscar sesiones abiertas
claude-pet sound [n]                  # prueba el grito del Pokémon n (sin n, el del sistema)
claude-pet focus <pid> [carpeta]      # diagnóstico: trae al frente la ventana de esa sesión
claude-pet quit
```

## Qué hace

| Evento (con los nombres de Claude Code) | Monito |
|---|---|
| SessionStart | aparece y cae desde arriba |
| UserPromptSubmit | camina rápido con "…" (trabajando) |
| Stop | salta, dice "¡Listo!" y lanza una notificación del sistema |
| Notification | agita los brazos con un "!" (permiso o atención) |
| SubagentStart / SubagentStop | nace una cría que lo sigue; se va cuando el subagente termina |
| SessionEnd, o si el proceso del agente muere | se despide y desaparece |

Al aterrizar se aplasta y al saltar se estira; al caminar rebota y en reposo respira. Sobre la cabeza muestra cómo está: "!" si te necesita, "?" si espera tu respuesta, "♪" al festejar, una gota de sudor si la tarea lleva más de 3 minutos y "♥" si le pasas el cursor.

**Crías:** cada subagente (por ejemplo, un Explore de Claude Code) es una cría a la mitad de tamaño que camina detrás de su monito. Se ven hasta cuatro; si hay más, un "+N". Al nacer dicen su tipo y al irse "✓".

**Entre ellos:**
- **Saludo:** dos monitos que se cruzan se detienen, se miran y se saludan. Cada uno espera un minuto y medio para volver a hacerlo.
- **Festejo contagioso:** cuando uno termina, los vecinos libres dan un saltito con él.
- **Quién te llama:** si uno pide atención, los que están quietos voltean a verlo.

**Contigo:**
- **Clic:** globo con el proyecto, el agente, su estado y sus crías.
- **Doble clic:** trae al frente su sesión. En Terminal e iTerm2 selecciona la pestaña exacta (macOS pide permiso de Automatización la primera vez); en VS Code, Cursor y parecidos abre la ventana del repo; en otras apps, la app. Si te está pidiendo atención, basta un clic. En Windows trae la ventana de la consola, sin elegir pestaña.
- **Arrastrar y soltar:** lo lanzas. Si choca con otro monito, lo tumba. Si lo sueltas sobre una ventana, aterriza en ella.
- **Clic derecho:** cambiar de estilo, solo ese monito: Base → Pokémon → los que agregues → Base. Toma un avatar del nuevo estilo que no traiga otro monito y lo dice en un globo.
- **⌥ Option + clic** (Alt + clic en Windows) **o clic central:** el siguiente avatar de su estilo, solo el de ese monito.
- **⇧ Shift + clic:** dormir o despertar.

Lo que eliges monito por monito se recuerda para su proyecto; elegir un avatar desde el menú borra esas elecciones.
- **Pasar el cursor encima:** se detiene y se pone feliz. A veces te persigue.

**Con la pantalla:**
- **Ventanas:** usan el borde de arriba de cualquier ventana no maximizada como plataforma. Saltan a ella, caminan encima y se tiran por la orilla. Si mueves la ventana, se van con ella. Si la cierras, la minimizas o la tapas con otra, se caen.
- **Bordes:** caminan sobre la barra de tareas o el Dock, trepan las paredes, andan por el techo y pasan de un monitor a otro.

**Bandeja / barra de menú:** monito de prueba, Avatar, Grupo del aleatorio, Ícono de la barra, Tamaño, ocultar, notificaciones on/off, sonidos on/off, buscar actualización, reiniciar, salir. La configuración se guarda en `config.json` junto al binario instalado.

- **Avatar:** uno fijo para todos los monitos, o "Aleatorio" (uno distinto por sesión). Agrupados por estilo y, en los Pokémon, por generación.
- **Grupo del aleatorio:** qué avatares entran en el sorteo (y por los que pasan el clic derecho y ⌥+clic). Cada uno se quita o se añade con su palomita; "Todo Base", "Todo Pokémon"… meten o sacan un estilo completo, "Toda la generación N" la generación completa y "Todos" vuelve a incluirlos todos. Si quitas todos, se sortea entre todos.
- **Sonidos:** cada Pokémon suena con su grito al aparecer, al terminar, al pedir permiso y, más bajito, al nacer una cría. El Monito y los avatares sin grito usan sonidos del sistema. Nunca suena más de uno cada 1.5 s, el permiso de un mismo monito suena a lo más cada 20 s, y las sesiones que la app encuentra al arrancar o reiniciar no suenan.
- **Ícono de la barra:** qué avatar se dibuja en la barra de menú o la bandeja. Por defecto, un Pokémon del grupo al azar en cada arranque (el Monito solo si no hay avatares cargados); elige uno para fijarlo.

## Agentes compatibles

La app no sabe de ningún agente en particular: escucha cinco eventos por TCP local (`SessionStart`, `UserPromptSubmit`, `Stop`, `Notification`, `SessionEnd`) y `claude-pet hook --agent <nombre>` traduce lo que manda cada CLI. `claude-pet install` registra los hooks en todos los que encuentre (por su carpeta de configuración); con `--agent` se limita a uno.

| Agente | Dónde se instala | Eventos que se traducen |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | los cinco tal cual, más `SubagentStart`/`SubagentStop` |
| Codex CLI | `~/.codex/hooks.json` | `PermissionRequest` → Notification, más `SubagentStart`/`SubagentStop` |
| Gemini CLI | `~/.gemini/settings.json` | `BeforeAgent` → UserPromptSubmit, `AfterAgent` → Stop |
| OpenCode | plugin en `~/.config/opencode/plugins/claude-pet.ts` | `session.created`, `chat.message`, `session.idle`, `permission.asked`, `session.deleted`; las sesiones hijas son crías |

- **Claude Desktop:** las sesiones de la pestaña **Code** usan los mismos hooks de `~/.claude/settings.json`, así que también tienen monito, con "Claude Desktop" en el globo y en los avisos. El monito queda atado al proceso de esa sesión (`CLAUDE_PID`) y el doble clic trae al frente Claude Desktop. Los chats normales de Claude Desktop no tienen hooks y no tienen monito.
- **Codex** no ejecuta hooks que no hayas aprobado: tras instalar, abre Codex y ejecuta `/hooks` para aprobar el de claude-pet. Si lo saltas no habrá monito ni ningún error visible. Si el comando cambia (por ejemplo, al reinstalar en otra ruta) hay que aprobarlo de nuevo, y cada evento nuevo (como `SubagentStart` y `SubagentStop` para las crías) también pide su aprobación.
- **Sesiones de Codex ya abiertas:** Codex solo dispara hooks al empezar un turno, así que una sesión abierta o reanudada a la que no le has escrito no avisaría. En macOS la app las busca sola al arrancar, al reiniciar y cada 15 s: toma cada proceso `codex` de terminal (no el daemon `codex app-server`), su carpeta, y el rollout más reciente de `~/.codex/sessions` con esa carpeta. El monito queda atado a ese proceso y se va cuando cierras Codex. En Windows esto no está disponible (el sistema no da la carpeta de otro proceso) y el monito aparece con el primer prompt.
- **OpenCode** no tiene hooks de shell, así que va un plugin que recibe los eventos en proceso y ejecuta el binario. Ignora las sesiones de subagentes y el "idle" inicial de cada sesión (no es un "¡Listo!"), y manda el PID de OpenCode para que el monito se vaya cuando lo cierres.
- **Otros CLI con hooks** (Cursor, Copilot, Kiro…): configura a mano un hook de tipo comando que ejecute `"/ruta/claude-pet" hook --agent <nombre>` con el JSON del evento por stdin (`hook_event_name`, `session_id`, `cwd`). Los nombres de evento que no se reconocen pasan tal cual, así que sirve cualquiera que coincida con los cinco de arriba.
- El monito muestra el agente en el globo al hacer clic y en los avisos ("Codex terminó"). Para ver qué llega, `CLAUDE_PET_DEBUG=1` escribe cada evento en `claude-pet.log`.

## Avatares

Los avatares van en estilos:

- **Base:** el Monito y 23 dibujados a mano (Gato, Perro, Robot, Fantasma, Alien, Panda, Ninja, Astronauta…). Vienen en el binario (`assets/base.json`).
- **Pokémon:** los baja `install` (abajo).
- **Los que agregues:** cada estilo es un archivo en `estilos/` dentro de la carpeta de datos (`~/Library/Application Support/claude-pet/` en macOS, `%LOCALAPPDATA%\claude-pet\` en Windows), junto a `config.json`. Aparecen en el menú y en el clic derecho en orden de nombre de archivo, después de Base.

`claude-pet install` deja listos los 809 Pokémon de las generaciones 1 a 7, con nombre en español y su grito: los baja de [PokeAPI](https://pokeapi.co) en tu máquina (unos 13 MB, menos de un minuto), sin Python. De la 8 en adelante PokeAPI no tiene iconos de este estilo. Si ya estaban no los vuelve a bajar; `claude-pet pokemon` lo fuerza. Quien se actualiza con el botón del menú y no los tiene los recibe al arrancar la app, en segundo plano y con un aviso al terminar; sin internet, lo intenta en el siguiente arranque. `install --no-pokemon` lo evita.

Para agregar un estilo:

```sh
claude-pet skins mi-archivo.json
```

Valida el archivo, lo guarda como `estilos/<estilo>.json` (si ya había uno de ese estilo, lo reemplaza) y, si la app está corriendo, recarga los menús sin reiniciar. Un estilo con más de 30 avatares se agrupa por generación (según el `id` de cada uno). Por defecto cada sesión recibe un avatar distinto del grupo del aleatorio; las elecciones (avatar fijo, grupo, ícono, avatar por proyecto) se guardan por nombre en `config.json`, así que sobreviven a un archivo nuevo. El `skins.json` de versiones anteriores pasa solo a `estilos/`.

Los de Pokémon los generan `claude-pet pokemon` y `tools/pokedex.py` (ver abajo), pero un estilo también se puede escribir a mano:

```json
{
  "style": "Pokémon",
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

- `style`: el nombre del estilo en el menú. Sin él, "Pokémon" si los avatares traen `id`, y si no "Personalizados".
- `rows`: `grid` filas de `grid` caracteres; `.` es transparente y los pies van en la última fila. Una fila con la mitad de caracteres se refleja (sprite simétrico). Si `grid` es menor que 24, el sprite se centra y se alinea al suelo.
- `palette`: un color `[r, g, b]` por carácter.
- `eyes` (opcional): celdas `[x, y]` que se cierran al parpadear y al dormir. También sirve `"eye": "k"` para marcar todas las celdas de un carácter.
- `id` (opcional): número de Pokédex, solo para agrupar el menú.
- `facing` (opcional): `"left"` si el dibujo mira a la izquierda, como los iconos de la Pokédex. La app lo voltea al cargar, porque asume que todo mira a la derecha y lo voltea al caminar hacia la izquierda.

Las poses (parpadeo, caminar, salto, sentado, dormido) y el contorno se generan solos. Un avatar mal formado se omite (con `CLAUDE_PET_DEBUG=1` queda en el log) y no tumba la app. `cargo test` valida los estilos instalados y genera `target/avatars.png` y `target/avatars_big.png` para verlos.

### Pokédex

`install` y `claude-pet pokemon` hacen lo mismo que `tools/pokedex.py` (`src/pokedex.rs`). El script sirve para escoger cuáles, cambiar la rejilla o los colores y revisar el resultado antes de instalarlo. Convierte los iconos de la Pokédex de [PokeAPI](https://github.com/PokeAPI/sprites) en avatares y necesita Python 3 y Pillow:

```sh
pip install pillow                                   # o: uv pip install pillow
python3 tools/pokedex.py --gen 1 --preview kanto.png  # los 151 de Kanto
python3 tools/pokedex.py --ids 25,133,143             # solo algunos (también rangos: 1-9)
claude-pet skins skins.json
```

Por cada Pokémon descarga el icono de Generación VII (pixel art de 40×30), lo recorta, lo reduce solo si no cabe en 24×24 (nunca se amplía), deja a lo más 12 colores (`--colors`), lo centra con los pies en la última fila y busca los ojos (manchas chicas del color más oscuro en la parte de arriba) para que parpadee; `--no-eyes` lo desactiva. También baja su grito en Ogg Vorbis a `cries/` junto al JSON (unos 7 KB cada uno; `--no-cries` lo evita), y `claude-pet skins` copia esa carpeta. Los nombres salen en español de la API (`--no-names` los evita). Las descargas se guardan en `tools/.cache/`. Con `--preview` escribe una hoja PNG para revisar el resultado antes de instalarlo.

Los sprites son propiedad de Nintendo, Game Freak y Creatures; PokeAPI los distribuye para uso personal. Por eso ni el repo ni el binario incluyen ningún sprite: cada instalación los baja para sí (y `.gitignore` ya excluye `skins.json`, los previews y la caché).

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

Los releases los publica el CI solo, a partir de la versión de `Cargo.toml`. Para sacar la versión `0.4.0`, sube `version = "0.4.0"` en `Cargo.toml` dentro del mismo PR del cambio (o en uno aparte) y fusiónalo a `main`. No hace falta crear tags a mano.

En cada push a `main`, el CI (pestaña **Actions**) compila en macOS y Windows y, si todavía no existe el tag `v0.4.0`, crea el tag y el release con `claude-pet-macos` y `claude-pet-windows.exe`. Si la versión no cambió, no publica nada: **un PR que fusionas sin subir la versión no llega a los usuarios**.

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
src/skins.rs        estilos de avatares: Base (assets/base.json) y los de estilos/
src/pokedex.rs      baja los Pokémon de PokeAPI y los convierte en avatares
src/hook_client.rs  `claude-pet hook --agent …`: traduce el evento del agente y lo manda a la app
src/codex.rs        encuentra sesiones de Codex abiertas que todavía no mandaron hooks
src/sound.rs        gritos (Ogg Vorbis → WAV en memoria) y sonidos del sistema
src/installer.rs    install/uninstall de hooks por agente
assets/opencode/    plugin de OpenCode (se copia con la ruta del binario)
src/platform/       Windows (layered windows, Win32) y macOS (AppKit/CoreGraphics)
```

Con `CLAUDE_PET_DEBUG=1`, la app escribe un log en la carpeta temporal (`claude-pet.log`).
