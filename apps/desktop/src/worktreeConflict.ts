import type { BranchWorktree, ProjectBranches } from './types';

export type ConflictChoice = 'reuse' | 'suffix' | 'replace';

export interface WorktreeConflict {
  name: string;
  worktree: BranchWorktree | null;
  choices: ConflictChoice[];
  suffixed: string | null;
}

const MAX_SUFFIX = 100;

export function detectConflict(
  branches: ProjectBranches | null,
  mode: 'new' | 'checkout',
  branch: string,
  checkout: string,
): WorktreeConflict | null {
  const name = (mode === 'new' ? branch : checkout).trim();
  if (!name || !branches) return null;
  const worktree = branches.worktrees.find((w) => w.dir === name) ?? null;
  const branchTaken = mode === 'new' && branches.local.includes(name);
  if (!worktree && !branchTaken) return null;
  return mode === 'new'
    ? { name, worktree, choices: ['reuse', 'suffix', 'replace'], suffixed: nextFree(name, branches) }
    : { name, worktree, choices: ['reuse', 'replace'], suffixed: null };
}

// Preview only; the daemon picks the actual name.
function nextFree(name: string, b: ProjectBranches): string | null {
  const taken = new Set([...b.local, ...b.worktrees.map((w) => w.dir)]);
  for (let n = 2; n <= MAX_SUFFIX; n++) if (!taken.has(`${name}-${n}`)) return `${name}-${n}`;
  return null;
}

export const replaceBlocked = (c: WorktreeConflict) => (c.worktree?.tasks.length ?? 0) > 0;
