import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ tasks: [] as unknown[], projects: [] as unknown[] }));
vi.mock('./api', () => ({
  api: {
    projects: async () => mocks.projects,
    workspaces: async () => [
      { id: 1, name: 'a', label: 'A', position: 1, repos: '', worktrees: '', color: '' },
      { id: 2, name: 'b', label: 'B', position: 2, repos: '', worktrees: '', color: '' },
    ],
    tasks: async () => mocks.tasks,
    sessions: async () => [],
    agents: async () => [],
    prList: async () => [],
    plugins: async () => [],
  },
  errorMessage: String,
}));

import { refresh, state } from './store';

describe('refresh', () => {
  beforeEach(() => {
    mocks.projects = [
      { id: 1, workspace_id: 1 },
      { id: 2, workspace_id: 2 },
    ];
    mocks.tasks = [
      { id: 10, project_id: 1 },
      { id: 20, project_id: 2 },
    ];
    state.activeWorkspace = 1;
    state.workspaceTasks = {};
  });

  it('drops a selected task from another workspace', async () => {
    state.selectedTaskId = 20;
    await refresh();
    expect(state.selectedTaskId).toBeNull();
  });

  it('keeps a selected task in the active workspace', async () => {
    state.selectedTaskId = 10;
    await refresh();
    expect(state.selectedTaskId).toBe(10);
  });
});
