import type { PrHit, TaskCreateRequest, TaskIssue } from './types';

const ADJECTIVES = ['two', 'slimy', 'quiet', 'brave', 'tiny', 'odd', 'swift', 'calm', 'bold', 'lucky', 'fuzzy', 'mellow', 'shiny', 'witty', 'sunny', 'rusty'];
const NOUNS = ['hairs', 'laws', 'clouds', 'foxes', 'pens', 'waves', 'stones', 'birds', 'lamps', 'trees', 'socks', 'maps', 'kites', 'bells', 'seeds', 'moons'];
const VERBS = ['begin', 'tan', 'jump', 'sing', 'drift', 'glow', 'spin', 'wander', 'nap', 'bloom', 'hum', 'race', 'dance', 'rest', 'wave', 'roam'];
const SUFFIX_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789';

const pick = <T>(list: T[], rand: () => number) => list[Math.floor(rand() * list.length)];

/** Matches the daemon's slug rules; a trailing dash survives so typing a space feels natural. */
export function liveSlug(input: string): string {
  return input.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+/, '');
}

export function finalSlug(input: string): string {
  return liveSlug(input).replace(/-+$/, '');
}

export function randomSlug(rand: () => number = Math.random): string {
  return [pick(ADJECTIVES, rand), pick(NOUNS, rand), pick(VERBS, rand)].join('-');
}

export function randomSuffix(rand: () => number = Math.random): string {
  return Array.from({ length: 5 }, () => pick([...SUFFIX_CHARS], rand)).join('');
}

export function defaultBranch(slug: string, suffix: string): string {
  return `asterism/${slug}-${suffix}`;
}

export function worktreeDir(branch: string): string {
  return branch.split('/').pop() ?? branch;
}

export function filterBranches(branches: string[], query: string): string[] {
  const q = query.trim().toLowerCase();
  return q ? branches.filter((b) => b.toLowerCase().includes(q)) : branches;
}

export function prDisabledReason(hit: PrHit): string | null {
  return hit.from_fork ? 'PR from a fork — not supported yet' : null;
}

export function issueStateClass(state: string): 'open' | 'progress' | 'done' | 'canceled' {
  const s = state.toLowerCase();
  if (/cancel/.test(s)) return 'canceled';
  if (/done|closed|complete|merged/.test(s)) return 'done';
  if (/progress|started|review/.test(s)) return 'progress';
  return 'open';
}

export interface TaskForm {
  projectId: number;
  title: string;
  placeholder: string;
  prompt: string;
  /** Empty means shell only. */
  agent: string;
  mode: 'new' | 'checkout';
  base: string;
  branch: string;
  checkout: string;
  push: boolean;
  issue: TaskIssue | null;
}

export function createRequest(f: TaskForm): TaskCreateRequest {
  const common = {
    project_id: f.projectId,
    title: finalSlug(f.title) || f.placeholder,
    prompt: (f.agent && f.prompt.trim()) || null,
    agent: f.agent || null,
    issue: f.issue,
  };
  return f.mode === 'checkout'
    ? { ...common, base: null, branch: null, checkout: f.checkout, push: false }
    : { ...common, base: f.base || null, branch: f.branch.trim() || null, checkout: null, push: f.push };
}
