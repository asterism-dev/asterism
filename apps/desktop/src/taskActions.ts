import { ask } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from './api';
import { toast } from './store';
import type { Task } from './types';

const report = (e: unknown) => toast(errorMessage(e));

export function archiveTask(task: Task) {
  api.archiveTask(task.id).catch(report);
}

export function restoreTask(task: Task) {
  api.restoreTask(task.id).catch(report);
}

export async function deleteTask(task: Task) {
  try {
    const check = await api.deleteCheck(task.id);
    const shared = check.shared_with.length > 0;
    const note = shared
      ? ` Its worktree stays: ${check.shared_with.length} other task(s) use it.`
      : check.dirty
        ? ' Its worktree has uncommitted changes, which will be lost.'
        : '';
    if (!(await ask(`Delete "${task.title}"?${note}`, { title: 'Delete task', kind: 'warning' })))
      return;
    let deleteBranch = false;
    if (check.branch_exists && !shared) {
      const unmerged = check.unmerged_commits
        ? ` It has ${check.unmerged_commits} commit(s) not on ${task.base_branch}.`
        : '';
      deleteBranch = await ask(`Also delete branch ${check.branch}?${unmerged}`, {
        title: 'Delete branch',
        kind: 'warning',
      });
    }
    await api.deleteTasks([task.id], deleteBranch);
  } catch (e) {
    report(e);
  }
}

// Sequential: each call runs git against the same repo, and one failure shouldn't stop the rest.
async function eachTask(tasks: Task[], verb: string, run: (t: Task) => Promise<unknown>) {
  const problems: string[] = [];
  for (const task of tasks) {
    try {
      const warning = await run(task);
      if (typeof warning === 'string') problems.push(`${task.title}: ${warning}`);
    } catch (e) {
      problems.push(`${task.title}: ${errorMessage(e)}`);
    }
  }
  if (problems.length)
    toast(`${verb} ${tasks.length} task(s) with problems — ${problems.join('; ')}`);
}

export const archiveTasks = (tasks: Task[]) =>
  eachTask(tasks, 'Archived', (t) => api.archiveTask(t.id));

export const restoreTasks = (tasks: Task[]) =>
  eachTask(tasks, 'Restored', (t) => api.restoreTask(t.id));

/** Returns once the daemon has started; progress shows up as an activity. */
export const deleteTasks = (tasks: Task[], deleteBranches: boolean) =>
  api
    .deleteTasks(
      tasks.map((t) => t.id),
      deleteBranches,
    )
    .catch(report);
