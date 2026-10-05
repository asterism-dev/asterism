import { describe, expect, it } from 'vitest';
import {
  SIDEBAR, floatKey, keptSizes, parseSidebar, parseWorkspace, placementPosition, reconcile, sessionIdOf, sessionPanelId, staleKeys,
  toggleAction, workspaceKey,
} from './model';

describe('layout model', () => {
  it('round-trips session panel ids', () => {
    expect(sessionPanelId(7)).toBe('session-7');
    expect(sessionIdOf('session-7')).toBe(7);
    expect(sessionIdOf('diff')).toBeNull();
    expect(sessionIdOf('session-x')).toBeNull();
  });

  it('keeps tool panes, drops panels of gone sessions and adds new sessions in order', () => {
    expect(reconcile(['session-1', 'diff', 'session-2', 'stray', 'activity'], [2, 3, 4])).toEqual({ remove: ['session-1', 'stray'], add: [3, 4] });
    expect(reconcile([], [])).toEqual({ remove: [], add: [] });
  });

  it('toggles closed → open, background → activate, front → close', () => {
    expect(toggleAction('closed')).toBe('open');
    expect(toggleAction('background')).toBe('activate');
    expect(toggleAction('front')).toBe('close');
  });

  it('parses only stored dockview layouts', () => {
    expect(parseWorkspace(null)).toBeNull();
    expect(parseWorkspace('not json')).toBeNull();
    expect(parseWorkspace('[]')).toBeNull();
    expect(parseWorkspace('{"leftWidth":260}')).toBeNull();
    expect(parseWorkspace('{"grid":{"root":{}},"panels":{}}')).toEqual({ grid: { root: {} }, panels: {} });
  });

  it('finds layout and float keys of tasks that no longer exist', () => {
    const keys = [workspaceKey(1), workspaceKey(2), floatKey(1), floatKey(2), 'asterism.theme', 'asterism.layoutTemplate'];
    expect(staleKeys(keys, [2])).toEqual([workspaceKey(1), floatKey(1)]);
  });

  it('places a session where asked, else in the focused, a session or a new left group', () => {
    const groups = [{ id: 'tools', hasSession: false }, { id: 'chat', hasSession: true }];
    expect(placementPosition({ referenceGroup: 'tools', direction: 'below' }, groups, null)).toEqual({ referenceGroup: 'tools', direction: 'below' });
    expect(placementPosition({ referenceGroup: 'gone', direction: 'within' }, groups, null)).toEqual({ referenceGroup: 'chat', direction: 'within' });
    expect(placementPosition(undefined, groups, 'tools')).toEqual({ referenceGroup: 'tools', direction: 'within' });
    expect(placementPosition(undefined, [{ id: 'tools', hasSession: false }], null)).toEqual({ referenceGroup: 'tools', direction: 'left' });
    expect(placementPosition(undefined, [], null)).toBeUndefined();
  });

  it('keeps every surviving group but the largest, which absorbs the freed space', () => {
    const before = [{ id: 'a', width: 260, height: 800 }, { id: 'w', width: 700, height: 800 }, { id: 'd', width: 420, height: 800 }];
    expect(keptSizes(before, ['a', 'w'])).toEqual([{ id: 'a', width: 260, height: 800 }]);
    expect(keptSizes(before, [])).toEqual([]);
  });

  it('reads the sidebar column with clamped width', () => {
    expect(parseSidebar(null)).toEqual({ width: SIDEBAR.initial, open: true });
    expect(parseSidebar('{"width":9999,"open":false}')).toEqual({ width: SIDEBAR.max, open: false });
    expect(parseSidebar('garbage')).toEqual({ width: SIDEBAR.initial, open: true });
  });
});
