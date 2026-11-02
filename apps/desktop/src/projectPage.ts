import { aggregate } from './store';
import type { Session, SessionStatus, Task, TaskDeleteCheck, Worktree } from './types';

export type TaskFilter = 'all' | 'active' | 'archived';

export function filterTasks(tasks: Task[], filter: TaskFilter): Task[] {
  return filter === 'all' ? tasks : tasks.filter((t) => t.archived === (filter === 'archived'));
}

export function taskState(
  task: Task,
  sessions: Session[],
): 'archived' | SessionStatus | 'no sessions' {
  if (task.archived) return 'archived';
  return (
    aggregate(sessions.filter((s) => s.task_id === task.id).map((s) => s.status)) ?? 'no sessions'
  );
}

export function worktreeActions(w: Worktree): { open: boolean; remove: boolean } {
  return { open: w.task_ids.length > 0, remove: !w.is_main && w.task_ids.length === 0 };
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

export function deleteWarnings(checks: TaskDeleteCheck[]): string[] {
  const count = (pick: (c: TaskDeleteCheck) => boolean) => checks.filter(pick).length;
  const dirty = count((c) => c.dirty && !c.shared_with.length);
  const shared = count((c) => c.shared_with.length > 0);
  const unmerged = count((c) => c.branch_exists && c.unmerged_commits > 0);
  return [
    dirty && `${dirty} with uncommitted changes, which will be lost.`,
    shared && `${shared} sharing a worktree with another task — it stays.`,
    unmerged && `${unmerged} with commits not on their base branch.`,
  ].filter((w): w is string => !!w);
}
