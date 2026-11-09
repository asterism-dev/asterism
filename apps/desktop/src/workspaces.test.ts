import { describe, expect, it } from 'vitest';
import type { Project, Session, Task, Workspace } from './types';
import { resolveWorkspace, waitingByWorkspace } from './workspaces';

const ws_ = (id: number, position: number): Workspace => ({
  id,
  name: `w${id}`,
  label: `W${id}`,
  position,
  repos: '',
  worktrees: '',
});

describe('resolveWorkspace', () => {
  it('falls back to the first workspace when the stored one is gone', () => {
    const ws = [ws_(2, 1), ws_(1, 0)];
    expect(resolveWorkspace(ws, 2)).toBe(2);
    expect(resolveWorkspace(ws, 99)).toBe(1);
    expect(resolveWorkspace(ws, null)).toBe(1);
    expect(resolveWorkspace([], 1)).toBeNull();
  });
});

describe('waitingByWorkspace', () => {
  it('counts waiting sessions per workspace', () => {
    const projects = [
      { id: 1, workspace_id: 1 },
      { id: 2, workspace_id: 2 },
    ] as Project[];
    const tasks = [
      { id: 1, project_id: 1 },
      { id: 2, project_id: 2 },
      { id: 3, project_id: 2 },
    ] as Task[];
    const waiting = [
      { id: 10, task_id: 2 },
      { id: 11, task_id: 3 },
    ] as Session[];
    expect(waitingByWorkspace(projects, tasks, waiting)).toEqual({ 2: 2 });
  });
});
