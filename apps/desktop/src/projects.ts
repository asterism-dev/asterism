import type { GithubStatus, Visibility } from './types';

export const COLLAPSED_KEY = 'asterism.collapsedProjects';

export function loadCollapsed(): Record<number, boolean> {
  try {
    const ids: unknown = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]');
    return Array.isArray(ids) ? Object.fromEntries(ids.filter((id) => typeof id === 'number').map((id) => [id, true])) : {};
  } catch {
    return {};
  }
}

export function saveCollapsed(collapsed: Record<number, boolean>) {
  try {
    localStorage.setItem(COLLAPSED_KEY, JSON.stringify(Object.keys(collapsed).filter((id) => collapsed[Number(id)]).map(Number)));
  } catch {
    // Storage can be unavailable; collapsing still works for this run.
  }
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

/** Mirrors the daemon's parser for previews; the daemon stays authoritative. */
export function sourceOwnerRepo(source: string): { owner: string; repo: string } | null {
  const trimmed = source.trim();
  let owner: string | undefined;
  let repo: string | undefined;
  const path = urlPath(trimmed);
  if (path !== null) {
    const segments = path.replace(/\/+$/, '').replace(/\.git$/, '').split('/').filter(Boolean);
    [owner, repo] = segments.slice(-2);
    if (segments.length < 2) return null;
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

export function visibilityChoices(owner: string, status: GithubStatus): Visibility[] {
  return status.orgs.includes(owner) && owner !== status.login ? ['private', 'public', 'internal'] : ['private', 'public'];
}
