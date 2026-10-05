import { ask } from '@tauri-apps/plugin-dialog';
import { api, errorMessage } from './api';
import { floatKey, workspaceKey } from './dock/model';
import { remove } from './dock/storage';
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
    const dirtyNote = check.dirty ? ' Its worktree has uncommitted changes, which will be lost.' : '';
    if (!(await ask(`Delete "${task.title}"?${dirtyNote}`, { title: 'Delete task', kind: 'warning' }))) return;
    let deleteBranch = false;
    if (check.branch_exists) {
      const unmerged = check.unmerged_commits ? ` It has ${check.unmerged_commits} commit(s) not on ${task.base_branch}.` : '';
      deleteBranch = await ask(`Also delete branch ${check.branch}?${unmerged}`, { title: 'Delete branch', kind: 'warning' });
    }
    const result = await api.deleteTask(task.id, deleteBranch);
    remove(workspaceKey(task.id));
    remove(floatKey(task.id));
    if (result.warning) toast(`Deleted with a warning: ${result.warning}`);
  } catch (e) {
    report(e);
  }
}
