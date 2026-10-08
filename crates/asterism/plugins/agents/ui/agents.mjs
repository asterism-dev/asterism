import { buildTree, formatDuration } from './tree.mjs';

const pending = new Map();
let nextId = 1;
let sessions = [];

function call(method, params) {
  const id = nextId++;
  parent.postMessage({ id, method, params }, '*');
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

function applyTheme(theme) {
  for (const [name, value] of Object.entries(theme.vars)) document.documentElement.style.setProperty(name, value);
  document.documentElement.style.colorScheme = theme.name;
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function subagentItem(node, now) {
  const li = el('li');
  const row = el('div', 'row');
  const end = node.ended_at ?? now;
  const time = el('span', 'time', formatDuration(end - node.started_at));
  if (node.ended_at == null) time.dataset.started = String(node.started_at);
  row.append(el('span', `dot ${node.status}`), el('span', 'kind', node.kind || 'agent'),
    el('span', '', node.description), time);
  li.append(row);
  if (node.children.length) li.append(listOf(node.children, now));
  return li;
}

function listOf(nodes, now) {
  const ul = el('ul');
  ul.append(...nodes.map((n) => subagentItem(n, now)));
  return ul;
}

function render() {
  const now = Date.now() / 1000;
  const tree = document.getElementById('tree');
  const open = new Set([...tree.querySelectorAll('details[open]')].map((d) => d.dataset.session));
  tree.replaceChildren(...buildTree(sessions).map((s) => {
    const li = el('li');
    const head = el('div', 'row session');
    head.append(el('span', `dot ${s.status}`), el('span', '', s.label));
    head.addEventListener('click', () => call('ui.focusSession', { sessionId: s.id }));
    li.append(head);
    if (s.running.length) li.append(listOf(s.running, now));
    if (s.finished.length) {
      const details = el('details');
      details.dataset.session = String(s.id);
      details.open = open.has(String(s.id));
      details.append(el('summary', '', `${s.finished.length} finished`), listOf(s.finished, now));
      li.append(details);
    }
    return li;
  }));
  document.getElementById('empty').hidden = sessions.length > 0;
}

function onEvent(event, data) {
  if (event === 'theme') return applyTheme(data);
  if (event === 'session.changed') {
    const old = sessions.find((s) => s.id === data.id);
    sessions = old ? sessions.map((s) => (s.id === data.id ? { ...s, ...data } : s)) : [...sessions, { ...data, subagents: [] }];
  } else if (event === 'session.status_changed') {
    sessions = sessions.map((s) => (s.id === data.session_id ? { ...s, status: data.status } : s));
  } else if (event === 'session.removed') {
    sessions = sessions.filter((s) => s.id !== data.session_id);
  } else if (event === 'subagent.started' || event === 'subagent.updated') {
    sessions = sessions.map((s) => s.id !== data.session_id ? s
      : { ...s, subagents: [...s.subagents.filter((x) => x.id !== data.subagent.id), data.subagent] });
  }
  render();
}

window.addEventListener('message', (e) => {
  if (e.source !== parent) return;
  const message = e.data;
  if (message && message.id !== undefined && pending.has(message.id)) {
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    return 'error' in message ? reject(message.error) : resolve(message.result);
  }
  if (message && typeof message.event === 'string') onEvent(message.event, message.data);
});

const context = await call('context');
applyTheme(context.theme);
await call('events.subscribe');
sessions = await call('sessions.list');
render();
// Only touch the running timers: rebuilding the tree every second would swallow clicks.
setInterval(() => {
  const now = Date.now() / 1000;
  for (const time of document.querySelectorAll('.time[data-started]')) {
    time.textContent = formatDuration(now - Number(time.dataset.started));
  }
}, 1000);
