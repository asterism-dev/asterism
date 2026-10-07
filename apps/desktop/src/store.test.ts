import { describe, expect, it } from 'vitest';
import {
  activeTab,
  aggregate,
  applyEvent,
  applyPrList,
  initialState,
  nextWaiting,
  selectSession,
  taskSessions,
  taskStatus,
  waitingSessions,
} from './store';
import type { PullRequest, Session, SessionStatus, Task } from './types';

const task = (id: number, projectId = 1): Task => ({
  id,
  project_id: projectId,
  title: `t${id}`,
  slug: `${id}`,
  branch: `asterism/${id}`,
  base_branch: 'main',
  worktree_path: `/wt/${id}`,
  prompt: null,
  issue: null,
  archived: false,
  created_at: 0,
  last_activity_at: 0,
});
const session = (id: number, taskId: number, status: SessionStatus = 'working'): Session => ({
  id,
  task_id: taskId,
  kind: { type: 'shell' },
  status,
});

describe('aggregate', () => {
  it('ranks waiting over working over idle over exited', () => {
    expect(aggregate([])).toBeNull();
    expect(aggregate(['idle', 'working'])).toBe('working');
    expect(aggregate(['exited', 'waiting_input', 'idle'])).toBe('waiting_input');
    expect(aggregate(['exited'])).toBe('exited');
  });

  it('rolls statuses up to tasks', () => {
    const s = initialState();
    s.tasks = [task(1), task(2)];
    s.sessions = [
      session(10, 1, 'idle'),
      session(11, 2, 'waiting_input'),
      session(12, 2, 'working'),
    ];
    expect(taskStatus(s, 1)).toBe('idle');
    expect(taskStatus(s, 2)).toBe('waiting_input');
    expect(taskStatus(s, 99)).toBeNull();
  });
});

describe('applyEvent', () => {
  it('reports a session only when it starts waiting', () => {
    const s = initialState();
    s.tasks = [task(1)];
    s.sessions = [session(10, 1)];
    const event = {
      method: 'session.status_changed',
      params: { session_id: 10, status: 'waiting_input' },
    } as const;
    expect(applyEvent(s, event)?.id).toBe(10);
    expect(applyEvent(s, event)).toBeNull();
    expect(s.sessions[0].status).toBe('waiting_input');
  });

  it('upserts sessions, tasks and projects', () => {
    const s = initialState();
    applyEvent(s, { method: 'session.changed', params: session(10, 1) });
    applyEvent(s, { method: 'session.changed', params: session(10, 1, 'idle') });
    applyEvent(s, { method: 'task.changed', params: task(1) });
    applyEvent(s, {
      method: 'project.changed',
      params: { id: 1, name: 'repo', path: '/repo', created_at: 0, default_base: null },
    });
    expect(s.sessions).toEqual([session(10, 1, 'idle')]);
    expect(s.tasks).toEqual([task(1)]);
    expect(s.projects).toHaveLength(1);
  });

  it('leaves the project page when its project is removed', () => {
    const s = initialState();
    s.projectPage = 3;
    applyEvent(s, { method: 'project.removed', params: { project_id: 4 } });
    expect(s.projectPage).toBe(3);
    applyEvent(s, { method: 'project.removed', params: { project_id: 3 } });
    expect(s.projectPage).toBeNull();
  });

  it('drops removed tasks and bumps the task version', () => {
    const s = initialState();
    s.tasks = [task(1)];
    s.sessions = [session(10, 1)];
    applyEvent(s, { method: 'task.removed', params: { task_id: 1 } });
    expect(s.tasks).toEqual([]);
    expect(s.sessions).toEqual([]);
    expect(s.tasksVersion).toBe(1);
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
    s.projects = [
      { id: 1, name: 'a', path: '/a', created_at: 0, default_base: null },
      { id: 2, name: 'b', path: '/b', created_at: 0, default_base: null },
    ];
    s.tasks = [task(1, 1), task(2, 2)];
    applyEvent(s, { method: 'project.removed', params: { project_id: 1 } });
    expect(s.projects.map((p) => p.id)).toEqual([2]);
    expect(s.tasks.map((t) => t.id)).toEqual([2]);
  });
});

describe('tabs and waiting sessions', () => {
  it("lists a task's sessions by id", () => {
    const s = initialState();
    s.sessions = [session(1, 7), session(2, 7), session(3, 7), session(4, 8)];
    expect(taskSessions(s, 7).map((x) => x.id)).toEqual([1, 2, 3]);
  });

  it('cycles through waiting sessions of known tasks', () => {
    const s = initialState();
    s.tasks = [task(1), task(2)];
    s.sessions = [
      session(10, 1, 'waiting_input'),
      session(11, 2, 'waiting_input'),
      session(12, 99, 'waiting_input'),
    ];
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

describe('session removal', () => {
  it('drops the session and its tab state so the next tab is chosen', () => {
    const s = initialState();
    s.tasks = [task(1)];
    s.sessions = [session(10, 1), session(11, 1)];
    s.selectedTaskId = 1;
    s.selectedTab[1] = 10;
    s.tabOrder[1] = [10, 11];
    applyEvent(s, { method: 'session.removed', params: { session_id: 10 } });
    expect(s.sessions.map((x) => x.id)).toEqual([11]);
    expect(s.tabOrder[1]).toEqual([11]);
    expect(s.selectedTab[1]).toBeUndefined();
    expect(activeTab(s, 1)).toBe(11);
    expect(applyEvent(s, { method: 'session.removed', params: { session_id: 99 } })).toBeNull();
  });
});

describe('collapsed projects', () => {
  it('expands the project of a session that is jumped to', () => {
    const s = initialState();
    s.tasks = [task(1, 7)];
    s.collapsed[7] = true;
    selectSession(s, session(10, 1, 'waiting_input'));
    expect(s.collapsed[7]).toBeUndefined();
    expect(s.selectedTaskId).toBe(1);
  });
});

describe('pull requests', () => {
  it('applies pr.changed and project lists', () => {
    const s = initialState();
    const p: PullRequest = {
      number: 3,
      url: 'u',
      title: 't',
      state: 'open',
      review: 'none',
      checks: { state: 'success', failing: [] },
    };
    applyEvent(s, { method: 'pr.changed', params: { task_id: 1, pr: p } });
    expect(s.prs[1].number).toBe(3);
    applyEvent(s, { method: 'pr.changed', params: { task_id: 1, pr: null } });
    expect(s.prs[1]).toBeUndefined();
    applyPrList(
      s,
      {
        prs: [{ task_id: 2, branch: 'b', pr: p }],
        errors: [{ project_id: 5, message: 'gh: not logged in' }],
      },
      5,
    );
    expect([s.prs[2].number, s.prErrors[5]]).toEqual([3, 'gh: not logged in']);
    applyPrList(s, { prs: [], errors: [] }, 5);
    expect(s.prErrors[5]).toBeUndefined();
  });
});
