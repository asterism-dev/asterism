import type { ProjectBranches } from './types';

export interface BaseChoice {
  options: string[];
  selected: string;
  hint: string | null;
  canCreate: boolean;
}

export function baseChoice(b: ProjectBranches | null): BaseChoice {
  if (!b) return { options: [], selected: '', hint: null, canCreate: false };
  if (!b.default)
    return {
      options: b.branches,
      selected: '',
      hint: 'This repository has no commits yet.',
      canCreate: false,
    };
  const options = b.branches.includes(b.default) ? b.branches : [b.default, ...b.branches];
  const hint = b.fetch_error ? "Couldn't fetch origin — branches may be stale." : null;
  return { options, selected: b.default, hint, canCreate: true };
}

/** The configured default base when it no longer resolves (the daemon then falls back to automatic). */
export function staleDefault(b: ProjectBranches | null): string | null {
  return b?.configured && b.default !== b.configured ? b.configured : null;
}
