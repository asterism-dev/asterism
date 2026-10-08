import { describe, expect, it } from 'vitest';
import {
  SIDEBAR,
  filePanelId,
  fileTaskOf,
  floatKey,
  isPluginPanel,
  keptSizes,
  parseSidebar,
  parseWorkspace,
  placementPosition,
  pluginPanelId,
  reconcile,
  sessionIdOf,
  sessionPanelId,
  staleKeys,
  toggleAction,
  workspaceKey,
} from './model';

describe('layout model', () => {
  it('round-trips session panel ids', () => {
    expect(sessionPanelId(7)).toBe('session-7');
    expect(sessionIdOf('session-7')).toBe(7);
    expect(sessionIdOf('diff')).toBeNull();
    expect(sessionIdOf('session-x')).toBeNull();
  });

  it('keeps tool panes, drops panels of gone sessions and adds new sessions in order', () => {
    expect(
      reconcile(['session-1', 'diff', 'session-2', 'stray', 'activity'], [2, 3, 4], 1),
    ).toEqual({ remove: ['session-1', 'stray'], add: [3, 4] });
    expect(reconcile([], [], 1)).toEqual({ remove: [], add: [] });
  });

  it('keeps file panes of the current task and drops those of others', () => {
    expect(reconcile(['file:1:src/a.ts', 'file:2:b.ts', 'session-3'], [3], 1)).toEqual({
      remove: ['file:2:b.ts'],
      add: [],
    });
  });

  it('keeps plugin panels, whatever the plugin', () => {
    const { remove } = reconcile([pluginPanelId('agents', 'agents'), 'stray'], [], 1);
    expect(remove).toEqual(['stray']);
    expect(isPluginPanel('plugin:agents/agents')).toBe(true);
    expect(isPluginPanel('session-1')).toBe(false);
  });

  it('round-trips file panel ids, including paths with colons', () => {
    expect(filePanelId(7, '~/x:y.md')).toBe('file:7:~/x:y.md');
    expect(fileTaskOf('file:7:~/x:y.md')).toBe(7);
    expect(fileTaskOf('session-7')).toBeNull();
    expect(fileTaskOf('diff')).toBeNull();
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
    expect(parseWorkspace('{"grid":{"root":{}},"panels":{}}')).toEqual({
      grid: { root: {} },
      panels: {},
    });
  });

  it('finds layout and float keys of tasks that no longer exist', () => {
    const keys = [
      workspaceKey(1),
      workspaceKey(2),
      floatKey(1),
      floatKey(2),
      'asterism.theme',
      'asterism.layoutTemplate',
    ];
    expect(staleKeys(keys, [2])).toEqual([workspaceKey(1), floatKey(1)]);
  });

  it('places a session where asked, else in the focused, a session or a new left group', () => {
    const groups = [
      { id: 'tools', hasSession: false },
      { id: 'chat', hasSession: true },
    ];
    expect(
      placementPosition({ referenceGroup: 'tools', direction: 'below' }, groups, null),
    ).toEqual({ referenceGroup: 'tools', direction: 'below' });
    expect(
      placementPosition({ referenceGroup: 'gone', direction: 'within' }, groups, null),
    ).toEqual({ referenceGroup: 'chat', direction: 'within' });
    expect(placementPosition(undefined, groups, 'tools')).toEqual({
      referenceGroup: 'tools',
      direction: 'within',
    });
    expect(placementPosition(undefined, [{ id: 'tools', hasSession: false }], null)).toEqual({
      referenceGroup: 'tools',
      direction: 'left',
    });
    expect(placementPosition(undefined, [], null)).toBeUndefined();
  });

  it('keeps every surviving group but the largest, which absorbs the freed space', () => {
    const before = [
      { id: 'a', width: 260, height: 800 },
      { id: 'w', width: 700, height: 800 },
      { id: 'd', width: 420, height: 800 },
    ];
    expect(keptSizes(before, ['a', 'w'])).toEqual([{ id: 'a', width: 260, height: 800 }]);
    expect(keptSizes(before, [])).toEqual([]);
  });

  it('reads the sidebar column with clamped width', () => {
    expect(parseSidebar(null)).toEqual({ width: SIDEBAR.initial, open: true });
    expect(parseSidebar('{"width":9999,"open":false}')).toEqual({
      width: SIDEBAR.max,
      open: false,
    });
    expect(parseSidebar('garbage')).toEqual({ width: SIDEBAR.initial, open: true });
  });
});
