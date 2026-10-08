import { describe, expect, it } from 'vitest';
import {
  ACTIONS,
  bindingFromEvent,
  findAction,
  findConflict,
  findResetConflict,
  formatBinding,
  isUsable,
  parseOverrides,
  resolveBindings,
  type Binding,
} from './shortcuts';

const ev = (p: Partial<KeyboardEvent>) =>
  ({
    key: '',
    code: '',
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...p,
  }) as KeyboardEvent;
const mac = resolveBindings({}, true);
const other = resolveBindings({}, false);
const ctrlK: Binding = {
  code: 'KeyK',
  label: 'K',
  meta: false,
  ctrl: true,
  alt: false,
  shift: false,
};

describe('shortcuts', () => {
  it('matches the macOS defaults', () => {
    expect(findAction(ev({ key: 'n', code: 'KeyN', metaKey: true }), mac, true, false)).toBe(
      'newTask',
    );
    expect(
      findAction(ev({ key: 'N', code: 'KeyN', metaKey: true, shiftKey: true }), mac, true, false),
    ).toBe('newProject');
    expect(findAction(ev({ key: ',', code: 'Comma', metaKey: true }), mac, true, false)).toBe(
      'settings',
    );
    expect(findAction(ev({ key: 'f', code: 'KeyF', metaKey: true }), mac, true, false)).toBe(
      'search',
    );
    expect(findAction(ev({ key: 'j', code: 'KeyJ', metaKey: true }), mac, true, false)).toBe(
      'nextWaiting',
    );
    expect(findAction(ev({ key: 'b', code: 'KeyB', metaKey: true }), mac, true, false)).toBe(
      'toggleSidebar',
    );
    expect(
      findAction(ev({ key: '∫', code: 'KeyB', metaKey: true, altKey: true }), mac, true, false),
    ).toBe('toggleDiff');
    expect(findAction(ev({ key: 'Enter', code: 'Enter', metaKey: true }), mac, true, false)).toBe(
      'confirm',
    );
  });

  it('matches the other-platform defaults', () => {
    const cs = { ctrlKey: true, shiftKey: true };
    expect(findAction(ev({ key: 'N', code: 'KeyN', ...cs }), other, false, false)).toBe('newTask');
    expect(findAction(ev({ key: 'P', code: 'KeyP', ...cs }), other, false, false)).toBe(
      'newProject',
    );
    expect(findAction(ev({ key: ',', code: 'Comma', ctrlKey: true }), other, false, false)).toBe(
      'settings',
    );
    expect(findAction(ev({ key: 'B', code: 'KeyB', ...cs }), other, false, false)).toBe(
      'toggleSidebar',
    );
    expect(
      findAction(ev({ key: 'B', code: 'KeyB', altKey: true, ...cs }), other, false, false),
    ).toBe('toggleDiff');
    expect(
      findAction(ev({ key: 'Enter', code: 'Enter', ctrlKey: true }), other, false, false),
    ).toBe('confirm');
  });

  it('requires an exact modifier match', () => {
    expect(findAction(ev({ key: 'n', code: 'KeyN' }), mac, true, false)).toBeNull();
    expect(
      findAction(ev({ key: 'n', code: 'KeyN', metaKey: true, ctrlKey: true }), mac, true, false),
    ).toBeNull();
    expect(
      findAction(ev({ key: 'n', code: 'KeyN', ctrlKey: true }), other, false, false),
    ).toBeNull();
  });

  it('treats numpad enter like return', () => {
    expect(
      findAction(ev({ key: 'Enter', code: 'NumpadEnter', metaKey: true }), mac, true, false),
    ).toBe('confirm');
  });

  it('ignores modifier-only key presses', () => {
    expect(bindingFromEvent(ev({ key: 'Meta', code: 'MetaLeft', metaKey: true }))).toBeNull();
    expect(bindingFromEvent(ev({ key: 'Shift', code: 'ShiftLeft', shiftKey: true }))).toBeNull();
  });

  it('applies overrides and unbound actions', () => {
    const b = resolveBindings({ newTask: ctrlK, search: null }, true);
    expect(findAction(ev({ key: 'k', code: 'KeyK', ctrlKey: true }), b, true, false)).toBe(
      'newTask',
    );
    expect(findAction(ev({ key: 'n', code: 'KeyN', metaKey: true }), b, true, false)).toBeNull();
    expect(findAction(ev({ key: 'f', code: 'KeyF', metaKey: true }), b, true, false)).toBeNull();
    expect(b.settings).toEqual(ACTIONS.settings.mac);
  });

  it('only lets terminal-safe bindings fire in a terminal', () => {
    const b = resolveBindings({ newTask: ctrlK }, true);
    expect(findAction(ev({ key: 'k', code: 'KeyK', ctrlKey: true }), b, true, true)).toBeNull();
    expect(findAction(ev({ key: 'b', code: 'KeyB', metaKey: true }), b, true, true)).toBe(
      'toggleSidebar',
    );
    expect(
      findAction(ev({ key: ',', code: 'Comma', ctrlKey: true }), other, false, true),
    ).toBeNull();
    expect(
      findAction(ev({ key: 'J', code: 'KeyJ', ctrlKey: true, shiftKey: true }), other, false, true),
    ).toBe('nextWaiting');
  });

  it('labels keys from e.key, falling back to the physical key with Alt', () => {
    expect(bindingFromEvent(ev({ key: 'z', code: 'KeyY', ctrlKey: true }))?.label).toBe('Z');
    expect(bindingFromEvent(ev({ key: '∫', code: 'KeyB', altKey: true }))?.label).toBe('B');
    expect(
      bindingFromEvent(ev({ key: '!', code: 'Digit1', metaKey: true, shiftKey: true }))?.label,
    ).toBe('1');
    expect(bindingFromEvent(ev({ key: 'Enter', code: 'Enter', metaKey: true }))?.label).toBe('↩');
  });

  it('rejects bindings that would fire while typing', () => {
    const plain = bindingFromEvent(ev({ key: 'n', code: 'KeyN' }));
    const shifted = bindingFromEvent(ev({ key: 'N', code: 'KeyN', shiftKey: true }));
    expect(plain && isUsable(plain)).toBe(false);
    expect(shifted && isUsable(shifted)).toBe(false);
    expect(isUsable(ctrlK)).toBe(true);
  });

  it('parses stored overrides defensively', () => {
    expect(parseOverrides(null)).toEqual({});
    expect(parseOverrides('not json')).toEqual({});
    expect(parseOverrides('[1,2]')).toEqual({});
    expect(parseOverrides('"x"')).toEqual({});
    expect(
      parseOverrides(
        JSON.stringify({ bogus: ctrlK, newTask: { code: 'KeyK' }, search: null, settings: ctrlK }),
      ),
    ).toEqual({
      search: null,
      settings: ctrlK,
    });
  });

  it('finds conflicts with other actions only', () => {
    expect(findConflict(mac, ACTIONS.newTask.mac, 'settings')).toBe('newTask');
    expect(findConflict(mac, ACTIONS.newTask.mac, 'newTask')).toBeNull();
    expect(findConflict(mac, ctrlK, 'settings')).toBeNull();
  });

  it('reports which action holds a default before resetting to it', () => {
    const overrides = { newTask: ACTIONS.confirm.mac, confirm: null };
    expect(findResetConflict(overrides, 'confirm', true)).toBe('newTask');
    expect(findResetConflict({ confirm: null }, 'confirm', true)).toBeNull();
  });

  it('formats bindings per platform', () => {
    expect(formatBinding(ACTIONS.newProject.mac, true)).toBe('⇧⌘N');
    expect(formatBinding(ACTIONS.toggleDiff.mac, true)).toBe('⌥⌘B');
    expect(formatBinding(ACTIONS.toggleDiff.other, false)).toBe('Ctrl+Alt+Shift+B');
    expect(formatBinding(ACTIONS.confirm.other, false)).toBe('Ctrl+↩');
  });
});
