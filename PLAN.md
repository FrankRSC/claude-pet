# Plan: soporte multi-agente y avatares Pokémon

Fork de [FrankRSC/claude-pet](https://github.com/FrankRSC/claude-pet). Dos objetivos independientes, cada uno en PRs pequeños que se puedan revisar y fusionar por separado:

- **A. Multi-agente:** que el monito funcione con Codex CLI, OpenCode, Gemini CLI y cualquier CLI con hooks, no solo con Claude Code.
- **B. Avatares Pokémon:** reemplazar los 23 avatares dibujados a mano por Pokémon generados desde PokeAPI con `tools/pokedex.py`.

## Cómo está hoy (lo que importa para el plan)

- La app de escritorio es agnóstica: escucha líneas JSON en TCP `127.0.0.1:47823` (`src/ipc.rs`) y solo entiende cinco eventos: `SessionStart`, `UserPromptSubmit`, `Stop`, `Notification`, `SessionEnd`. Los campos que lee `on_hook` en `src/app.rs` son `hook_event_name`, `session_id`, `cwd`, `pet_pid` y `message`.
- Lo específico de Claude Code vive en tres sitios:
  - `src/installer.rs`: escribe los hooks solo en `~/.claude/settings.json`.
  - `src/hook_client.rs` + `find_ancestor` en `src/platform/mod.rs`: lee el JSON por stdin y busca el PID subiendo por los procesos padre hasta uno llamado `claude*` (o `node*` como respaldo).
  - `src/app.rs`: vigila `~/.claude/sessions/*.json`, y usa textos fijos ("Claude terminó", "Claude necesita tu atención").
- Avatares: `Skin { name, rows: [&str; 16], palette, eye }` en `src/skins.rs`. `sprites::build` genera las 8 poses (parpadeo, caminar, salto, sentado, dormido) desplazando filas y cerrando los ojos. `GRID = 16` está en `src/sprites.rs` y lo usan `render.rs` (buffer y rotaciones) y `pet.rs` (`size() = cell_px × GRID`, con `cell_px` de 3 a 8, 5 por defecto). El menú de la bandeja lista `sprites::count()` avatares y `config.json` guarda la elección por nombre.

---

## Parte A: multi-agente

### Qué expone cada agente (verificado en la documentación oficial, octubre 2026)

| Agente | Configuración | Entrada del hook | Eventos útiles |
|---|---|---|---|
| Claude Code | `~/.claude/settings.json` → `hooks` | JSON por stdin: `session_id`, `cwd`, `hook_event_name` | Los cinco actuales |
| Codex CLI | `~/.codex/hooks.json` o tabla `[hooks]` en `~/.codex/config.toml` | Mismo contrato por stdin (`session_id`, `cwd`, `hook_event_name`, `model`) | `SessionStart`, `UserPromptSubmit`, `Stop`, `PermissionRequest`, `SessionEnd` |
| Gemini CLI | `~/.gemini/settings.json` → `hooks` | Mismo contrato por stdin, más `timestamp` | `SessionStart`, `BeforeAgent`, `AfterAgent`, `Notification`, `SessionEnd` |
| OpenCode | Plugin JS/TS en `~/.config/opencode/plugins/` | No hay hooks de shell: el plugin recibe eventos en proceso | `session.created`, `session.idle`, `permission.asked`, `session.deleted`, hook `chat.message` |
| Cursor CLI, Copilot CLI, Kiro | Hooks propios con JSON por stdin | Variantes del mismo contrato | Mapear igual que Gemini |

Notas que condicionan el diseño:

- Codex: los hooks no gestionados deben aprobarse con `/hooks` dentro de Codex antes de que corran, y si cambian hay que volver a aprobarlos. El instalador no puede hacerlo; debe imprimir la instrucción.
- Codex también tiene `notify = [...]` en `config.toml`, pero solo dispara `agent-turn-complete` y pasa el JSON como argumento, no por stdin. Con los hooks ya no hace falta.
- Gemini: cada hook necesita `name`, y `timeout` va en milisegundos (en Claude va en segundos).
- OpenCode: el plugin puede ejecutar nuestro binario (`$` de Bun) o conectarse al puerto TCP. Ejecutar el binario reutiliza la lógica de "si la app no corre, la lanzo" de `hook_client.rs`.

### Mapeo de eventos a los cinco internos

| Interno | Claude Code | Codex | Gemini | OpenCode |
|---|---|---|---|---|
| `SessionStart` | `SessionStart` | `SessionStart` | `SessionStart` | `session.created` |
| `UserPromptSubmit` | `UserPromptSubmit` | `UserPromptSubmit` | `BeforeAgent` | hook `chat.message` |
| `Stop` | `Stop` | `Stop` | `AfterAgent` | `session.idle` |
| `Notification` | `Notification` | `PermissionRequest` | `Notification` | `permission.asked` |
| `SessionEnd` | `SessionEnd` | `SessionEnd` | `SessionEnd` | `session.deleted` |

### A1. Normalizar eventos y añadir `agent` al protocolo

Archivos: `src/hook_client.rs`, `src/app.rs`.

- `claude-pet hook --agent <nombre>`: el instalador pasa el agente en el comando del hook, así el cliente no tiene que adivinar.
- En `hook_client::run`, antes de mandar por IPC: renombrar `hook_event_name` según la tabla de arriba y añadir `"agent": "<nombre>"`. Mantener compatibilidad: sin `--agent`, se asume `claude`.
- En `on_hook`, guardar `agent` en el `Pet` (nuevo campo) y usarlo en textos: `"{agent} terminó"`, `"{agent} necesita tu atención"`, y en la etiqueta por defecto cuando no hay `cwd`.
- El monito que llega sin `pet_pid` ni padre reconocible debe seguir funcionando (hoy ya lo hace: `pid` es opcional).

### A2. Instalador por agente

Archivo: `src/installer.rs`.

- `claude-pet install [--agent claude|codex|gemini|opencode|all]`. Por defecto `all`: instala en cada agente cuya carpeta exista (`~/.claude`, `~/.codex`, `~/.gemini`, `~/.config/opencode`). `uninstall` es simétrico.
- Un `enum Agent` con tres funciones: `config_path()`, `install(&mut settings, command)`, `strip(&mut settings)`. El marcador `claude-pet` en el comando sigue sirviendo para reconocer nuestras entradas.
- Formatos a escribir:

  Claude Code (ya existe):
  ```json
  { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "\"/ruta/claude-pet\" hook --agent claude", "timeout": 10 } ] } ] } }
  ```

  Codex, en `~/.codex/hooks.json` (misma estructura por evento; confirmar la unidad de `timeout` en la referencia):
  ```json
  { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "\"/ruta/claude-pet\" hook --agent codex", "timeout": 10 } ] } ] } }
  ```
  Tras instalar, imprimir: "Abre Codex y ejecuta `/hooks` para aprobar el hook de claude-pet".

  Gemini, en `~/.gemini/settings.json`:
  ```json
  { "hooks": { "AfterAgent": [ { "matcher": "*", "hooks": [ { "name": "claude-pet", "type": "command", "command": "\"/ruta/claude-pet\" hook --agent gemini", "timeout": 5000 } ] } ] } }
  ```

  OpenCode: copiar `assets/opencode/claude-pet.ts` a `~/.config/opencode/plugins/claude-pet.ts` con la ruta del binario sustituida. Boceto (verificar los nombres de campos contra los tipos de `@opencode-ai/plugin`):
  ```ts
  import type { Plugin } from "@opencode-ai/plugin"
  const BIN = "__CLAUDE_PET_BIN__"
  export const ClaudePet: Plugin = async ({ $, directory }) => {
    const send = async (hook_event_name: string, session_id: string, extra = {}) => {
      const payload = JSON.stringify({ hook_event_name, session_id, cwd: directory, agent: "opencode", pet_pid: process.pid, ...extra })
      await $`${BIN} hook --agent opencode < ${new Response(payload)}`.quiet().nothrow()
    }
    return {
      event: async ({ event }) => {
        switch (event.type) {
          case "session.created": return send("SessionStart", event.properties.info.id)
          case "session.idle": return send("Stop", event.properties.sessionID)
          case "permission.asked": return send("Notification", event.properties.sessionID, { message: "Permiso pendiente" })
          case "session.deleted": return send("SessionEnd", event.properties.info.id)
        }
      },
      "chat.message": async (input) => send("UserPromptSubmit", input.sessionID),
    }
  }
  ```
  Como el plugin manda `pet_pid`, el cliente no tiene que buscar el proceso padre.

- `write_settings` ya hace respaldo `.bak`; reutilizarlo para cada archivo.

### A3. Detección del proceso

Archivo: `src/platform/mod.rs` (y `hook_client.rs`).

- `find_ancestor` recibe la lista de nombres a buscar según `--agent`: `["claude"]`, `["codex"]`, `["gemini", "node"]`, `["opencode"]`. Hoy ya acepta `node` como respaldo, que es lo que cubre a Gemini.
- Si el JSON de entrada ya trae `pet_pid`, no sobreescribirlo (hoy lo pisa si encuentra un ancestro).
- `pid_alive` no cambia: con el PID correcto, el monito desaparece cuando muere el proceso del agente, sea cual sea.

### A4. Detalles por agente en la app

Archivo: `src/app.rs`.

- El vigilante de `~/.claude/sessions` queda tal cual: es un extra solo para Claude. Los demás agentes solo entran por hook.
- Opcional: avatar por defecto según agente (por ejemplo, Pokémon de tipo fuego para Codex, de tipo planta para Gemini). Se resuelve en `pick_avatar` con el campo `agent`.
- El globo al hacer clic muestra `agent` junto al proyecto.

### A5. README y pruebas

- README: sección "Agentes compatibles" con el comando de instalación y el paso manual de Codex (`/hooks`).
- Prueba manual por agente: abrir sesión, mandar prompt, esperar `Stop`, provocar un permiso, cerrar. Verificar en `claude-pet.log` (con `CLAUDE_PET_DEBUG=1`) que llegan los cinco eventos con el `session_id` correcto.
- Test unitario para el mapeo de eventos (función pura en `hook_client.rs`).

---

## Parte B: avatares Pokémon

### B0. Decisión de rejilla: 24×24

Se probó el pipeline con los iconos de Generación VII de PokeAPI (40×30, pixel art con fondo transparente, cuerpo de 20 a 32 px). Resultado:

| Rejilla | Qué se ve |
|---|---|
| 16×16 | Se reconoce la silueta y el color (Pikachu, Gengar, Snorlax), pero Meowth o Magikarp quedan en manchas |
| 24×24 | Casi sin pérdida: la mayoría cabe a tamaño real o se reduce poco. Caras y detalles legibles |
| 32×32 | Idéntico al original, pero el sprite ocupa solo dos tercios del cuadro |

Recomendación: `GRID = 24`. Para mantener el tamaño en pantalla parecido al actual (16 × 5 = 80 px), bajar `cell_px` por defecto a 3 (72 px) y el rango a 2–5.

Cambios que implica:
- `src/sprites.rs`: `GRID = 24`. Las poses dibujadas a mano del Monito (`STAND`, `BLINK`, …, 16 filas de 16) se envuelven con una función `pad16` que las centra horizontalmente y las alinea al suelo. Lo mismo para `icon_rgba`.
- `src/pet.rs`: `MIN_CELL = 2`, `MAX_CELL = 5`, valor inicial 3. `src/app.rs` y `src/main.rs` muestran el rango en el menú y en `claude-pet size`.
- `src/render.rs`: `BUF` se recalcula solo a partir de `GRID` y `MAX_CELL`.
- Alternativa si no se quiere tocar el render: quedarse en 16 (el script ya lo soporta con `--grid 16`) aceptando la pérdida de detalle.

### B1. Script `tools/pokedex.py` (ya hay un prototipo funcional en el repo)

- Fuente: `sprites/pokemon/versions/generation-vii/icons/{id}.png` del repo de sprites de PokeAPI. No necesita la API para las imágenes; la API `pokemon-species/{id}` solo se usa para el nombre en español (con caché en `tools/.cache/`).
- Pipeline por Pokémon: recortar al bbox opaco → reducir con promedio por bloque ponderado por alpha (solo si no cabe; nunca se amplía) → alpha binario → cuantizar a ≤ 12 colores con median cut sin dither → centrar y alinear los pies a la última fila → filas de caracteres + paleta.
- Salida: `skins.json` con `{"grid": 24, "skins": [{"id", "name", "rows", "palette"}]}`. Con `--preview` genera una hoja PNG para revisar.
- Pendiente del prototipo: detectar ojos para el parpadeo (ver B2), y un flag `--shiny` si se quiere usar `front_shiny` (solo existe en los sprites de 96×96, habría que reducir más).

### B2. Carga de avatares en tiempo de ejecución

Archivos: `src/sprites.rs`, `src/skins.rs`, `src/app.rs`, `src/main.rs`.

- La app lee `<carpeta de datos>/claude-pet/skins.json` al arrancar (misma carpeta que `config.json`). Si no existe, solo queda el Monito.
- `Skin` pasa a tener campos propios (`String`, `Vec`) o se construye con `Box::leak` al cargar, para no cambiar las firmas `&'static`. `count()`, `name()` y `look()` recorren la lista cargada; el índice 0 sigue siendo el Monito.
- Validar al cargar lo mismo que el test `skins_are_well_formed`: filas de `GRID` caracteres y todos en la paleta. Un avatar inválido se omite con una línea en el log, no tumba la app.
- Ojos: el campo `eye` pasa a ser opcional. Sin ojo, las poses de parpadeo y dormido reutilizan el cuadro base. Mejora posterior: heurística en el script (pares verticales del color más oscuro en el tercio superior).
- `config.json` ya guarda `avatar` y `project_avatars` por nombre, así que no cambia el formato.
- Menú de la bandeja: con más de 30 avatares, agrupar en submenús por generación (el `id` lo determina). "Aleatorio" sigue eligiendo entre todos los cargados sin repetir.
- Nuevo comando `claude-pet skins <archivo.json>`: copia el archivo a la carpeta de datos y manda `{"cmd":"reload"}` por IPC para que la app lo recargue sin reiniciar.

### B3. Retirar los avatares dibujados a mano

- Borrar los 23 `Skin` de `src/skins.rs` (quedan en el historial de git). El Monito se mantiene como respaldo y como ícono de la bandeja.
- Adaptar los tests: `skins_are_well_formed` valida `skins.json` si existe; `preview_sheet` dibuja lo cargado.

### B4. Derechos de uso

Los sprites son propiedad de Nintendo, Game Freak y Creatures; PokeAPI los distribuye para uso personal. Para no tener problemas en un repo público: no subir `skins.json`, previews ni `tools/.cache/` (ya están en `.gitignore`), y que el README explique que cada quien los genera con el script.

### B5. Ideas para después (fuera de este plan)

- Evolución: tras N tareas terminadas el Pokémon evoluciona siguiendo `evolution-chain` de PokeAPI.
- Un Pokémon por agente (A4) o por proyecto, elegido automáticamente por el hash del nombre.
- Animación de caminar con los sprites animados de Generación V (96×96, muchos cuadros); requiere otro pipeline y más memoria.

---

## Orden propuesto y tamaño

| # | PR | Alcance | Tamaño |
|---|---|---|---|
| 1 | `feat/skins-runtime` | B0 + B2 + B3: rejilla 24, carga de `skins.json`, menú por generación, comando `skins` | M |
| 2 | `feat/pokedex` | B1: script terminado con ojos y README de avatares | S |
| 3 | `feat/agentes` | A1 + A2 + A3: protocolo con `agent`, instalador por agente, detección de PID, plugin de OpenCode | M |
| 4 | `feat/agentes-ui` | A4 + A5: textos por agente, avatar por agente, README, pruebas | S |

Los PRs 1–2 y 3–4 son independientes y pueden avanzar en paralelo. Cada uno debe pasar el CI de macOS y Windows (`cargo build --release` y `cargo test`).

## Riesgos

- Codex exige aprobar el hook con `/hooks`; si el usuario no lo hace, no verá monito y no habrá error visible. Mitigar con el mensaje al instalar y una línea en el README.
- Los nombres exactos de los campos de eventos de OpenCode (`event.properties.*`) hay que confirmarlos contra el SDK antes de dar el plugin por bueno.
- Cambiar `GRID` toca el Monito, el ícono de la bandeja y el tamaño en pantalla; hacerlo en un solo PR con capturas antes y después.
- Un menú con 151 o más entradas sin submenús es inusable; los submenús por generación son parte del PR 1, no opcionales.
