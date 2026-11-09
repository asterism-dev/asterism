import type { Project, Session, Task, Workspace } from './types';

const ACTIVE_KEY = 'asterism.activeWorkspace';
const TASKS_KEY = 'asterism.workspaceTasks';
const MORE_KEY = 'asterism.moreProjectsOpen';

export function resolveWorkspace(workspaces: Workspace[], wanted: number | null): number | null {
  if (wanted !== null && workspaces.some((w) => w.id === wanted)) return wanted;
  const first = [...workspaces].sort((a, b) => a.position - b.position || a.id - b.id)[0];
  return first?.id ?? null;
}

export function waitingByWorkspace(
  projects: Project[],
  tasks: Task[],
  waiting: Session[],
): Record<number, number> {
  const projectWs = new Map(projects.map((p) => [p.id, p.workspace_id]));
  const taskProject = new Map(tasks.map((t) => [t.id, t.project_id]));
  const counts: Record<number, number> = {};
  for (const s of waiting) {
    const ws = projectWs.get(taskProject.get(s.task_id) ?? -1);
    if (ws !== undefined) counts[ws] = (counts[ws] ?? 0) + 1;
  }
  return counts;
}

function loadMap<T>(key: string, valid: (v: unknown) => boolean): Record<number, T> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(key) ?? '{}');
    if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return {};
    return Object.fromEntries(Object.entries(raw).filter(([, v]) => valid(v))) as Record<number, T>;
  } catch {
    return {};
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage can be unavailable; state still works for this run.
  }
}

export function loadActiveWorkspace(): number | null {
  try {
    const id: unknown = JSON.parse(localStorage.getItem(ACTIVE_KEY) ?? 'null');
    return typeof id === 'number' ? id : null;
  } catch {
    return null;
  }
}

export function saveActiveWorkspace(id: number) {
  save(ACTIVE_KEY, id);
}

export const loadWorkspaceTasks = () => loadMap<number>(TASKS_KEY, (v) => typeof v === 'number');
export const saveWorkspaceTasks = (map: Record<number, number>) => save(TASKS_KEY, map);
export const loadMoreOpen = () => loadMap<boolean>(MORE_KEY, (v) => typeof v === 'boolean');
export const saveMoreOpen = (map: Record<number, boolean>) => save(MORE_KEY, map);

export function slugText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

export function defaultFolders(reposDefault: string, name: string) {
  const home = reposDefault.replace(/[\\/][^\\/]*$/, '');
  const slug = slugText(name) || 'workspace';
  return { repos: `${home}/${slug}/repos`, worktrees: `${home}/${slug}/worktrees` };
}
