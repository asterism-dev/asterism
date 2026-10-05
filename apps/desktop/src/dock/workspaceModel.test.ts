import { describe, expect, it } from 'vitest';
import { parseWorkspace, reconcile, sessionIdOf, sessionPanelId, staleWorkspaceKeys, targetGroup, workspaceKey } from './workspaceModel';

describe('workspace model', () => {
  it('round-trips session panel ids', () => {
    expect(sessionPanelId(7)).toBe('session-7');
    expect(sessionIdOf('session-7')).toBe(7);
    expect(sessionIdOf('diff')).toBeNull();
    expect(sessionIdOf('session-x')).toBeNull();
  });

  it('drops panels of gone sessions and adds new sessions in order', () => {
    expect(reconcile(['session-1', 'session-2', 'stray'], [2, 3, 4])).toEqual({ remove: ['session-1', 'stray'], add: [3, 4] });
    expect(reconcile([], [])).toEqual({ remove: [], add: [] });
  });

  it('prefers the last focused group if it still exists', () => {
    expect(targetGroup('g2', ['g1', 'g2'])).toBe('g2');
    expect(targetGroup('gone', ['g1', 'g2'])).toBe('g1');
    expect(targetGroup(null, [])).toBeUndefined();
  });

  it('parses only stored dockview layouts', () => {
    expect(parseWorkspace(null)).toBeNull();
    expect(parseWorkspace('[]')).toBeNull();
    expect(parseWorkspace('{"grid":{"root":{}},"panels":{}}')).toEqual({ grid: { root: {} }, panels: {} });
  });

  it('finds workspace keys of tasks that no longer exist', () => {
    const keys = [workspaceKey(1), workspaceKey(2), 'asterism.theme', 'asterism.workspace.x'];
    expect(staleWorkspaceKeys(keys, [2])).toEqual([workspaceKey(1), 'asterism.workspace.x']);
  });
});
