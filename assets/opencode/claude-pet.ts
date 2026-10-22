// Plugin de OpenCode para Claude Pet. OpenCode no tiene hooks de shell: el
// plugin recibe los eventos en proceso, los traduce a los que entiende la app
// y los manda con el binario (`claude-pet hook`), que arranca la app si no está
// corriendo. Lo instala `claude-pet install --agent opencode` en
// ~/.config/opencode/plugins/ con la ruta real en vez de __CLAUDE_PET_BIN__.

const BIN = "__CLAUDE_PET_BIN__"

export const ClaudePet = async ({ $, directory }) => {
  // Carpeta de cada sesión (es la etiqueta del monito).
  const dirs = new Map()
  // Sesiones hijas (subagentes) → { parent, done }: son crías del monito padre.
  const children = new Map()
  // Sesiones con un prompt en curso: `session.idle` también llega al crear la
  // sesión, y eso no es un "¡Listo!".
  const busy = new Set()

  const send = (hook_event_name, session_id, extra = {}) => {
    if (!session_id) return
    const payload = JSON.stringify({
      hook_event_name,
      session_id,
      cwd: dirs.get(session_id) ?? directory,
      agent: "opencode",
      pet_pid: process.pid,
      ...extra,
    })
    // Sin await: si tiene que arrancar la app tarda un par de segundos, y eso
    // no debe frenar la sesión. Un fallo del hook tampoco.
    $`${BIN} hook --agent opencode < ${new Response(payload)}`
      .quiet()
      .nothrow()
      .catch(() => {})
  }

  // La cría terminó (o la borraron): avisa una sola vez al padre. Devuelve si
  // `id` era una cría.
  const childDone = (id) => {
    const child = children.get(id)
    if (!child) return false
    if (!child.done) {
      child.done = true
      send("SubagentStop", child.parent, { agent_id: id })
    }
    return true
  }

  return {
    event: async ({ event }) => {
      const p = event.properties
      switch (event.type) {
        case "session.created":
          if (p.info.parentID) {
            children.set(p.info.id, { parent: p.info.parentID, done: false })
            return send("SubagentStart", p.info.parentID, {
              agent_id: p.info.id,
              agent_type: p.info.title || "subagente",
            })
          }
          dirs.set(p.info.id, p.info.directory)
          return send("SessionStart", p.info.id)
        case "session.idle":
          if (childDone(p.sessionID)) return
          if (busy.delete(p.sessionID)) return send("Stop", p.sessionID)
          return
        case "permission.asked": // OpenCode actual
        case "permission.updated": // SDK anterior a 2.x
          // El permiso de una cría lo pide su padre.
          return send("Notification", children.get(p.sessionID)?.parent ?? p.sessionID, {
            message: p.title ? `Permiso: ${p.title}` : "Permiso pendiente",
          })
        case "session.deleted":
          if (childDone(p.info.id)) return void children.delete(p.info.id)
          dirs.delete(p.info.id)
          busy.delete(p.info.id)
          return send("SessionEnd", p.info.id)
      }
    },
    "chat.message": async (input) => {
      if (children.has(input.sessionID)) return
      busy.add(input.sessionID)
      send("UserPromptSubmit", input.sessionID)
    },
  }
}
