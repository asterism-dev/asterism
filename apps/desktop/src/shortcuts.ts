import { computed, ref } from 'vue';

export type ActionId =
  | 'newTask'
  | 'newProject'
  | 'settings'
  | 'search'
  | 'nextWaiting'
  | 'toggleSidebar'
  | 'toggleDiff'
  | 'confirm';
export type GlobalActionId = Exclude<ActionId, 'confirm'>;

export interface Binding {
  code: string;
  label: string;
  meta: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
}
export type Overrides = Partial<Record<ActionId, Binding | null>>;
export type Bindings = Record<ActionId, Binding | null>;

type Mods = Partial<Pick<Binding, 'meta' | 'ctrl' | 'alt' | 'shift'>>;
const key = (code: string, label: string, mods: Mods): Binding => ({
  code,
  label,
  meta: false,
  ctrl: false,
  alt: false,
  shift: false,
  ...mods,
});
const CMD = { meta: true };
const CTRL_SHIFT = { ctrl: true, shift: true };

export const ACTIONS: Record<ActionId, { title: string; mac: Binding; other: Binding }> = {
  newTask: { title: 'New task', mac: key('KeyN', 'N', CMD), other: key('KeyN', 'N', CTRL_SHIFT) },
  newProject: {
    title: 'New project',
    mac: key('KeyN', 'N', { meta: true, shift: true }),
    other: key('KeyP', 'P', CTRL_SHIFT),
  },
  settings: {
    title: 'Settings',
    mac: key('Comma', ',', CMD),
    other: key('Comma', ',', { ctrl: true }),
  },
  search: {
    title: 'Search sidebar',
    mac: key('KeyF', 'F', CMD),
    other: key('KeyF', 'F', CTRL_SHIFT),
  },
  nextWaiting: {
    title: 'Next waiting session',
    mac: key('KeyJ', 'J', CMD),
    other: key('KeyJ', 'J', CTRL_SHIFT),
  },
  toggleSidebar: {
    title: 'Toggle sidebar',
    mac: key('KeyB', 'B', CMD),
    other: key('KeyB', 'B', CTRL_SHIFT),
  },
  toggleDiff: {
    title: 'Toggle review',
    mac: key('KeyB', 'B', { meta: true, alt: true }),
    other: key('KeyB', 'B', { ...CTRL_SHIFT, alt: true }),
  },
  confirm: {
    title: 'Confirm dialog',
    mac: key('Enter', '↩', CMD),
    other: key('Enter', '↩', { ctrl: true }),
  },
};
export const ACTION_IDS = Object.keys(ACTIONS) as ActionId[];

const STORAGE_KEY = 'asterism.shortcuts';
export const IS_MAC = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');

const MODIFIER_KEYS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Fn']);
const KEY_LABELS: Record<string, string> = {
  Enter: '↩',
  ' ': 'Space',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Backspace: '⌫',
  Delete: '⌦',
  Tab: '⇥',
  Escape: 'Esc',
};

function isBinding(v: unknown): v is Binding {
  if (typeof v !== 'object' || v === null) return false;
  const b = v as Record<string, unknown>;
  return (
    typeof b.code === 'string' &&
    b.code !== '' &&
    typeof b.label === 'string' &&
    ['meta', 'ctrl', 'alt', 'shift'].every((m) => typeof b[m] === 'boolean')
  );
}

export function parseOverrides(raw: string | null): Overrides {
  let data: unknown;
  try {
    data = JSON.parse(raw ?? '{}');
  } catch {
    return {};
  }
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return {};
  const out: Overrides = {};
  for (const id of ACTION_IDS) {
    if (!(id in data)) continue;
    const v = (data as Record<string, unknown>)[id];
    if (v === null || isBinding(v)) out[id] = v;
  }
  return out;
}

export function resolveBindings(overrides: Overrides, mac: boolean): Bindings {
  return Object.fromEntries(
    ACTION_IDS.map((id) => [
      id,
      id in overrides ? (overrides[id] ?? null) : ACTIONS[id][mac ? 'mac' : 'other'],
    ]),
  ) as Bindings;
}

const normalCode = (code: string) => (code === 'NumpadEnter' ? 'Enter' : code);

export function sameKeys(a: Binding, b: Binding): boolean {
  return (
    normalCode(a.code) === normalCode(b.code) &&
    a.meta === b.meta &&
    a.ctrl === b.ctrl &&
    a.alt === b.alt &&
    a.shift === b.shift
  );
}

export function bindingFromEvent(e: KeyboardEvent): Binding | null {
  if (MODIFIER_KEYS.has(e.key) || !e.code) return null;
  // ponytail: Alt alters e.key on macOS ("∫") and Shift digits, so these show the QWERTY key; non-QWERTY layouts may mislabel.
  const physical = /^(?:Key|Digit)(.)$/.exec(e.code)?.[1];
  const usePhysical = physical && (e.altKey || (e.shiftKey && e.code.startsWith('Digit')));
  const label =
    KEY_LABELS[e.key] ??
    (usePhysical ? physical : e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return {
    code: e.code,
    label,
    meta: e.metaKey,
    ctrl: e.ctrlKey,
    alt: e.altKey,
    shift: e.shiftKey,
  };
}

// Without ⌘, Ctrl or Alt a shortcut would fire while typing.
export const isUsable = (b: Binding) => b.meta || b.ctrl || b.alt;
export const terminalSafe = (b: Binding, mac: boolean) => (mac ? b.meta : b.ctrl && b.shift);

export function findAction(
  e: KeyboardEvent,
  bindings: Bindings,
  mac: boolean,
  inTerminal: boolean,
): ActionId | null {
  const pressed = bindingFromEvent(e);
  if (!pressed) return null;
  return (
    ACTION_IDS.find((id) => {
      const b = bindings[id];
      return b !== null && sameKeys(b, pressed) && (!inTerminal || terminalSafe(b, mac));
    }) ?? null
  );
}

export function findConflict(bindings: Bindings, b: Binding, except: ActionId): ActionId | null {
  return (
    ACTION_IDS.find((id) => {
      const other = bindings[id];
      return id !== except && other !== null && sameKeys(other, b);
    }) ?? null
  );
}

export function findResetConflict(
  overrides: Overrides,
  id: ActionId,
  mac: boolean,
): ActionId | null {
  return findConflict(resolveBindings(overrides, mac), ACTIONS[id][mac ? 'mac' : 'other'], id);
}

export function formatBinding(b: Binding, mac: boolean): string {
  if (mac)
    return `${b.ctrl ? '⌃' : ''}${b.alt ? '⌥' : ''}${b.shift ? '⇧' : ''}${b.meta ? '⌘' : ''}${b.label}`;
  return [b.ctrl && 'Ctrl', b.alt && 'Alt', b.shift && 'Shift', b.meta && 'Meta', b.label]
    .filter(Boolean)
    .join('+');
}

function load(): Overrides {
  try {
    return parseOverrides(localStorage.getItem(STORAGE_KEY));
  } catch {
    return {};
  }
}

const overrides = ref<Overrides>(load());
const bindings = computed(() => resolveBindings(overrides.value, IS_MAC));

function save(next: Overrides) {
  overrides.value = next;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage can be unavailable; the bindings still apply for this run.
  }
}

export const bindingFor = (id: ActionId) => bindings.value[id];
export const hasOverride = (id: ActionId) => id in overrides.value;
export const conflictFor = (b: Binding, except: ActionId) =>
  findConflict(bindings.value, b, except);

export function matchAction(e: KeyboardEvent, { inTerminal = false } = {}): ActionId | null {
  return findAction(e, bindings.value, IS_MAC, inTerminal);
}

export function setBinding(id: ActionId, b: Binding | null) {
  const next = { ...overrides.value };
  if (b && sameKeys(b, ACTIONS[id][IS_MAC ? 'mac' : 'other'])) delete next[id];
  else next[id] = b;
  save(next);
}

export const defaultBinding = (id: ActionId) => ACTIONS[id][IS_MAC ? 'mac' : 'other'];
export const resetConflict = (id: ActionId) => findResetConflict(overrides.value, id, IS_MAC);

export function resetBinding(id: ActionId) {
  const next = { ...overrides.value };
  delete next[id];
  save(next);
}

export const resetAll = () => save({});

export function shortcutHint(id: ActionId): string {
  const b = bindingFor(id);
  return b ? formatBinding(b, IS_MAC) : '';
}

export function withHint(title: string, id: ActionId): string {
  const hint = shortcutHint(id);
  return hint ? `${title} (${hint})` : title;
}
