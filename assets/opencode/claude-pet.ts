// Plugin de OpenCode 2 para Claude Pet. OpenCode no tiene hooks de shell: el
// plugin recibe los eventos en proceso (dentro del servicio `opencode serve`),
// los traduce a los que entiende la app y los manda con el binario
// (`claude-pet hook`), que arranca la app si no está corriendo. Lo instala
// `claude-pet install --agent opencode` en ~/.config/opencode/plugins/ con la
// ruta real en vez de __CLAUDE_PET_BIN__.
//
// Eventos de OpenCode 2 → los de la app:
//   session.created (sin parentID)          → SessionStart
//   session.created (con parentID)          → SubagentStart en la sesión padre
//   session.execution.started               → UserPromptSubmit
//   session.execution.succeeded/failed/
//     interrupted (no por apagado)          → Stop (o SubagentStop si es una cría)
//   permission.asked                        → Notification (la de una cría, en el padre)
//   session.deleted                         → SessionEnd (o SubagentStop)

const BIN = "__CLAUDE_PET_BIN__"

// OpenCode arranca una instancia del plugin por proyecto abierto y todas
// reciben todos los eventos: el estado es uno solo y cada evento se atiende
// una vez.
const state = (globalThis.__claudePet ??= {
  seen: new Set(),
  order: [],
  // sesión hija (subagente) → { parent, done }
  children: new Map(),
  // sesión → carpeta del proyecto (la etiqueta del monito)
  dirs: new Map(),
})

const firstTime = (id) => {
  if (!id) return true
  if (state.seen.has(id)) return false
  state.seen.add(id)
  state.order.push(id)
  if (state.order.length > 1000) state.seen.delete(state.order.shift())
  return true
}

const send = (hook_event_name, session_id, extra = {}) => {
  if (!session_id) return
  const payload = JSON.stringify({
    hook_event_name,
    session_id,
    cwd: state.dirs.get(session_id) ?? "",
    agent: "opencode",
    ...extra,
  })
  // Sin shell (las rutas con espacios no estorban) y sin esperar: arrancar la
  // app puede tardar y eso no debe frenar la sesión. Un fallo tampoco.
  try {
    Bun.spawn([BIN, "hook", "--agent", "opencode"], {
      stdin: new Blob([payload]),
      stdout: "ignore",
      stderr: "ignore",
    })
  } catch {}
}

// La cría terminó (o la borraron): avisa una sola vez al padre. Devuelve si
// la sesión era una cría.
const childDone = (id) => {
  const child = state.children.get(id)
  if (!child) return false
  if (!child.done) {
    child.done = true
    send("SubagentStop", child.parent, { agent_id: id })
  }
  return true
}

const handle = (event) => {
  if (!firstTime(event.id)) return
  const d = event.data ?? {}
  const id = d.sessionID
  const dir = d.location?.directory ?? event.location?.directory
  if (id && dir && !d.parentID && !state.children.has(id)) state.dirs.set(id, dir)
  switch (event.type) {
    case "session.created": {
      if (d.parentID) {
        state.children.set(id, { parent: d.parentID, done: false })
        const kind = d.title || (typeof d.agent === "string" ? d.agent : d.agent?.name) || "subagente"
        return send("SubagentStart", d.parentID, { agent_id: id, agent_type: kind })
      }
      return send("SessionStart", id)
    }
    case "session.execution.started":
      if (state.children.has(id)) return
      return send("UserPromptSubmit", id)
    case "session.execution.interrupted":
      // Al apagar OpenCode no hay nada que festejar.
      if (d.reason === "shutdown") return
    // fallthrough
    case "session.execution.succeeded":
    case "session.execution.failed":
      if (childDone(id)) return
      return send("Stop", id)
    case "permission.asked": {
      // El permiso de una cría lo pide su padre.
      const owner = state.children.get(id)?.parent ?? id
      return send("Notification", owner, { message: d.message ? `Permiso: ${d.message}` : "Permiso pendiente" })
    }
    case "session.deleted":
      if (childDone(id)) return void state.children.delete(id)
      send("SessionEnd", id)
      state.dirs.delete(id)
      return
  }
}

export default {
  id: "claude-pet",
  async setup(ctx) {
    const controller = new AbortController()
    ;(async () => {
      try {
        for await (const event of ctx.event.subscribe({ signal: controller.signal })) handle(event)
      } catch {}
    })()
    return () => controller.abort()
  },
}
