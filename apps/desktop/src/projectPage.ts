import { aggregate } from './store';
import type { Session, SessionStatus, Task, Worktree } from './types';

export type TaskFilter = 'all' | 'active' | 'archived';

export function filterTasks(tasks: Task[], filter: TaskFilter): Task[] {
  return filter === 'all' ? tasks : tasks.filter((t) => t.archived === (filter === 'archived'));
}

export function taskState(task: Task, sessions: Session[]): 'archived' | SessionStatus | 'no sessions' {
  if (task.archived) return 'archived';
  return aggregate(sessions.filter((s) => s.task_id === task.id).map((s) => s.status)) ?? 'no sessions';
}

export function worktreeActions(w: Worktree): { open: boolean; remove: boolean } {
  return { open: w.task_id !== null, remove: !w.is_main && w.task_id === null };
}

export function formatSize(bytes: number | undefined): string {
  if (bytes === undefined) return '…';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit === 0 ? `${value} B` : `${value.toFixed(1)} ${units[unit]}`;
}
