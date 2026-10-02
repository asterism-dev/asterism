import { describe, expect, it } from 'vitest';
import {
  aggregate, applyEvent, initialState, moveTab, nextWaiting, nodeAggregateStatus, projectStatus, taskSessions, taskStatus, waitingSessions,
} from './store';
import type { Session, SessionStatus, Task } from './types';

const task = (id: number, projectId = 1): Task => ({
  id, project_id: projectId, title: `t${id}`, slug: `${id}`, branch: `asterism/${id}`, base_branch: 'main',
  worktree_path: `/wt/${id}`, prompt: null, archived: false,
});
const session = (id: number, taskId: number, status: SessionStatus = 'working'): Session => ({
  id, task_id: taskId, kind: { type: 'shell' }, status,
});

describe('aggregate', () => {
  it('ranks waiting over working over idle over exited', () => {
    expect(aggregate([])).toBeNull();
    expect(aggregate(['idle', 'working'])).toBe('working');
    expect(aggregate(['exited', 'waiting_input', 'idle'])).toBe('waiting_input');
    expect(aggregate(['exited'])).toBe('exited');
  });

  it('rolls statuses up to tasks and projects', () => {
    const s = initialState();
    s.tasks = [task(1), task(2), task(3, 2)];
    s.sessions = [session(10, 1, 'idle'), session(11, 2, 'waiting_input'), session(12, 3, 'working')];
    expect(taskStatus(s, 1)).toBe('idle');
    expect(projectStatus(s, 1)).toBe('waiting_input');
    expect(projectStatus(s, 2)).toBe('working');
    expect(taskStatus(s, 99)).toBeNull();
  });
});

describe('applyEvent', () => {
  it('reports a session only when it starts waiting', () => {
    const s = initialState();
    s.tasks = [task(1)];
    s.sessions = [session(10, 1)];
    const event = { method: 'session.status_changed', params: { session_id: 10, status: 'waiting_input' } } as const;
    expect(applyEvent(s, event)?.id).toBe(10);
    expect(applyEvent(s, event)).toBeNull();
    expect(s.sessions[0].status).toBe('waiting_input');
  });

  it('upserts sessions, tasks and projects', () => {
    const s = initialState();
    applyEvent(s, { method: 'session.changed', params: session(10, 1) });
    applyEvent(s, { method: 'session.changed', params: session(10, 1, 'idle') });
    applyEvent(s, { method: 'task.changed', params: task(1) });
    applyEvent(s, { method: 'project.changed', params: { id: 1, name: 'repo', path: '/repo' } });
    expect(s.sessions).toEqual([session(10, 1, 'idle')]);
    expect(s.tasks).toEqual([task(1)]);
    expect(s.projects).toHaveLength(1);
  });

  it('drops archived tasks with their sessions and selection', () => {
    const s = initialState();
    s.tasks = [task(1), task(2)];
    s.sessions = [session(10, 1), session(11, 2)];
    s.selectedTaskId = 1;
    applyEvent(s, { method: 'task.changed', params: { ...task(1), archived: true } });
    expect(s.tasks.map((t) => t.id)).toEqual([2]);
    expect(s.sessions.map((x) => x.id)).toEqual([11]);
    expect(s.selectedTaskId).toBeNull();
  });

  it('removes a project and its tasks', () => {
    const s = initialState();
    s.projects = [{ id: 1, name: 'a', path: '/a' }, { id: 2, name: 'b', path: '/b' }];
    s.tasks = [task(1, 1), task(2, 2)];
    applyEvent(s, { method: 'project.removed', params: { project_id: 1 } });
    expect(s.projects.map((p) => p.id)).toEqual([2]);
    expect(s.tasks.map((t) => t.id)).toEqual([2]);
  });
});

describe('tabs and waiting sessions', () => {
  it('orders tabs by drag order, then by id', () => {
    const s = initialState();
    s.sessions = [session(1, 7), session(2, 7), session(3, 7), session(4, 8)];
    expect(taskSessions(s, 7).map((x) => x.id)).toEqual([1, 2, 3]);
    moveTab(s, 7, 3, 1);
    expect(taskSessions(s, 7).map((x) => x.id)).toEqual([3, 1, 2]);
    moveTab(s, 7, 3, 2);
    expect(taskSessions(s, 7).map((x) => x.id)).toEqual([1, 3, 2]);
  });

  it('cycles through waiting sessions of known tasks', () => {
    const s = initialState();
    s.tasks = [task(1), task(2)];
    s.sessions = [session(10, 1, 'waiting_input'), session(11, 2, 'waiting_input'), session(12, 99, 'waiting_input')];
    expect(waitingSessions(s).map((x) => x.id)).toEqual([10, 11]);
    expect(nextWaiting(s)?.id).toBe(10);
    s.selectedTaskId = 1;
    s.selectedTab[1] = 10;
    expect(nextWaiting(s)?.id).toBe(11);
    s.selectedTaskId = 2;
    s.selectedTab[2] = 11;
    expect(nextWaiting(s)?.id).toBe(10);
  });
});

describe('nodeAggregateStatus', () => {
  it('aggregates sessions of known tasks across projects', () => {
    const s = initialState();
    expect(nodeAggregateStatus(s)).toBeNull();
    s.tasks = [task(1, 1), task(2, 2)];
    s.sessions = [session(1, 1, 'idle'), session(2, 2, 'waiting_input'), session(3, 99, 'working')];
    expect(nodeAggregateStatus(s)).toBe('waiting_input');
  });
});
