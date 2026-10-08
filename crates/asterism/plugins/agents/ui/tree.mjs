const FINISHED = new Set(['done', 'failed', 'ended']);

export function formatDuration(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s}s`;
  const pad = (n) => String(n).padStart(2, '0');
  if (s < 3600) return `${Math.floor(s / 60)}m ${pad(s % 60)}s`;
  return `${Math.floor(s / 3600)}h ${pad(Math.floor((s % 3600) / 60))}m`;
}

function label(kind) {
  if (kind.type === 'agent') return kind.name;
  if (kind.type === 'command') return kind.argv[0] ?? 'command';
  return 'shell';
}

/** Sessions → [{id, label, status, running: Node[], finished: Node[]}]; Node = subagent + children. */
export function buildTree(sessions) {
  return sessions.map((session) => {
    const ids = new Set(session.subagents.map((s) => s.id));
    const nodes = new Map(session.subagents.map((s) => [s.id, { ...s, children: [] }]));
    const roots = [];
    for (const node of nodes.values()) {
      const parent = node.parent_id && ids.has(node.parent_id) ? nodes.get(node.parent_id) : null;
      (parent ? parent.children : roots).push(node);
    }
    return {
      id: session.id,
      label: label(session.kind),
      status: session.status,
      running: roots.filter((n) => !FINISHED.has(n.status)),
      finished: roots.filter((n) => FINISHED.has(n.status)),
    };
  });
}
