import type { ForgeStatus, Project, Task, Visibility } from './types';

export const COLLAPSED_KEY = 'asterism.collapsedProjects';

export function loadCollapsed(): Record<number, boolean> {
  try {
    const ids: unknown = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
    return Array.isArray(ids)
      ? Object.fromEntries(ids.filter((id) => typeof id === 'number').map((id) => [id, true]))
      : {};
  } catch {
    return {};
  }
}

export function saveCollapsed(collapsed: Record<number, boolean>) {
  try {
    localStorage.setItem(
      COLLAPSED_KEY,
      JSON.stringify(
        Object.keys(collapsed)
          .filter((id) => collapsed[Number(id)])
          .map(Number),
      ),
    );
  } catch {
    // Storage can be unavailable; collapsing still works for this run.
  }
}

export type SortMode = 'alphabetical' | 'activity' | 'added';
export const SORT_KEY = 'asterism.sortMode';

export function loadSortMode(): SortMode {
  try {
    const mode = localStorage.getItem(SORT_KEY);
    return mode === 'alphabetical' || mode === 'activity' ? mode : 'added';
  } catch {
    return 'added';
  }
}

export function saveSortMode(mode: SortMode) {
  try {
    localStorage.setItem(SORT_KEY, mode);
  } catch {
    // Storage can be unavailable; the order still applies for this run.
  }
}

const byName = (a: string, b: string) =>
  a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true });

export function sortTasks(tasks: Task[], mode: SortMode): Task[] {
  const key = (t: Task) => (mode === 'activity' ? t.last_activity_at : t.created_at);
  return [...tasks].sort(
    (a, b) => (mode === 'alphabetical' ? byName(a.title, b.title) : key(b) - key(a)) || b.id - a.id,
  );
}

export function projectActivity(p: Project, tasks: Task[]): number {
  return tasks.reduce(
    (latest, t) => (t.project_id === p.id ? Math.max(latest, t.last_activity_at) : latest),
    p.created_at,
  );
}

export function sortProjects(projects: Project[], tasks: Task[], mode: SortMode): Project[] {
  const key = (p: Project) => (mode === 'activity' ? projectActivity(p, tasks) : p.created_at);
  return [...projects].sort(
    (a, b) => (mode === 'alphabetical' ? byName(a.name, b.name) : key(b) - key(a)) || b.id - a.id,
  );
}

/** The project's tasks shown for a search, or null to hide the project; a matching project name keeps all its tasks. */
export function filterTasks(project: Project, tasks: Task[], query: string): Task[] | null {
  const q = query.trim().toLowerCase();
  const own = tasks.filter((t) => t.project_id === project.id);
  if (!q || project.name.toLowerCase().includes(q)) return own;
  const hits = own.filter(
    (t) => t.title.toLowerCase().includes(q) || t.branch.toLowerCase().includes(q),
  );
  return hits.length ? hits : null;
}

const UNITS: [number, string][] = [
  [31_536_000, 'y'],
  [604_800, 'w'],
  [86_400, 'd'],
  [3_600, 'h'],
  [60, 'm'],
];

export function relativeTime(seconds: number, now: number): string {
  const elapsed = Math.max(0, now - seconds);
  const unit = UNITS.find(([size]) => elapsed >= size);
  return unit ? `${Math.floor(elapsed / unit[0])}${unit[1]}` : 'now';
}

const SAFE_NAME = /^[A-Za-z0-9._-]+$/;

export function repoNameError(name: string): string | null {
  if (!name) return 'Enter a name.';
  if (name === '.' || name === '..' || name.startsWith('-') || !SAFE_NAME.test(name)) {
    return "Use letters, digits, '.', '_' or '-' (not starting with '-').";
  }
  return null;
}

function urlPath(source: string): string | null {
  const scheme = source.indexOf('://');
  if (scheme !== -1) {
    const rest = source.slice(scheme + 3);
    const slash = rest.indexOf('/');
    return slash === -1 ? null : rest.slice(slash);
  }
  const colon = source.indexOf(':');
  if (colon > 0 && !source.slice(0, colon).includes('/')) return source.slice(colon + 1);
  return null;
}

function isGithub(source: string): boolean {
  const rest = source.indexOf('://') !== -1 ? source.slice(source.indexOf('://') + 3) : source;
  const authority = rest.split(/[/:]/)[0] || '';
  return authority.split('@').pop() === 'github.com';
}

export function sourceOwnerRepo(source: string): { owner: string; repo: string } | null {
  const trimmed = source.trim();
  // eslint-disable-next-line no-control-regex -- rejecting control characters is the point
  if (trimmed.startsWith('-') || /[\s\x00-\x1f\x7f]/.test(trimmed)) return null;

  let owner: string | undefined;
  let repo: string | undefined;
  const path = urlPath(trimmed);
  if (path !== null) {
    const segments = path
      .replace(/\/+$/, '')
      .replace(/\.git$/, '')
      .split('/')
      .filter(Boolean);
    if (segments.length < 2) return null;
    if (isGithub(trimmed)) {
      [owner, repo] = segments.slice(0, 2);
    } else {
      [owner, repo] = segments.slice(-2);
    }
  } else {
    const parts = trimmed.split('/');
    if (parts.length !== 2) return null;
    [owner, repo] = parts;
  }
  if (!owner || !repo || repoNameError(owner) || repoNameError(repo)) return null;
  return { owner, repo };
}

export function targetPath(root: string, owner: string, repo: string): string {
  return `${root.replace(/\/+$/, '')}/${owner}/${repo}`;
}

export function visibilityChoices(owner: string, status: ForgeStatus): Visibility[] {
  const ownerLower = owner.toLowerCase();
  const accountLower = status.account?.toLowerCase();
  const isOrg = status.owners.some((org) => org.toLowerCase() === ownerLower);
  const isNotOwn = !accountLower || ownerLower !== accountLower;
  return isOrg && isNotOwn ? ['private', 'public', 'internal'] : ['private', 'public'];
}
