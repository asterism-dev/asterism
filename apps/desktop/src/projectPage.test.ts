import { describe, expect, it } from 'vitest';
import { filterTasks, formatSize, taskState, worktreeActions } from './projectPage';
import type { Session, Task, Worktree } from './types';

const task = (id: number, archived = false): Task => ({
  id, project_id: 1, title: `t${id}`, slug: `${id}`, branch: `b${id}`, base_branch: 'main', worktree_path: `/wt/${id}`,
  prompt: null, issue: null, archived, created_at: 0, last_activity_at: 0,
});
const wt = (over: Partial<Worktree>): Worktree => ({
  path: '/wt', head: 'abc', branch: 'b', is_main: false, locked: false, prunable: false, task_id: null, base_branch: null, ...over,
});

describe('project page', () => {
  it('filters tasks by state', () => {
    const tasks = [task(1), task(2, true)];
    expect(filterTasks(tasks, 'all').map((t) => t.id)).toEqual([1, 2]);
    expect(filterTasks(tasks, 'active').map((t) => t.id)).toEqual([1]);
    expect(filterTasks(tasks, 'archived').map((t) => t.id)).toEqual([2]);
  });

  it('derives a task row state from archive flag and sessions', () => {
    const sessions: Session[] = [{ id: 1, task_id: 1, kind: { type: 'shell' }, status: 'working' }];
    expect(taskState(task(2, true), sessions)).toBe('archived');
    expect(taskState(task(1), sessions)).toBe('working');
    expect(taskState(task(3), sessions)).toBe('no sessions');
  });

  it('offers open only for linked and remove only for unlinked side worktrees', () => {
    expect(worktreeActions(wt({ task_id: 4 }))).toEqual({ open: true, remove: false });
    expect(worktreeActions(wt({ is_main: true }))).toEqual({ open: false, remove: false });
    expect(worktreeActions(wt({}))).toEqual({ open: false, remove: true });
  });

  it('formats sizes and pending sizes', () => {
    expect(formatSize(undefined)).toBe('…');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(1536)).toBe('1.5 KB');
    expect(formatSize(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatSize(3 * 1024 ** 3)).toBe('3.0 GB');
  });
});
