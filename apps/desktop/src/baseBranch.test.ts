import { describe, expect, it } from 'vitest';
import { baseChoice, staleDefault } from './baseBranch';
import type { ProjectBranches } from './types';

const branches = (over: Partial<ProjectBranches> = {}): ProjectBranches => ({
  branches: ['main', 'origin/main'], default: 'origin/main', automatic: 'origin/main', configured: null, fetch_error: null, worktree_root: null, ...over,
});

describe('baseChoice', () => {
  it('is not creatable while loading', () => {
    expect(baseChoice(null)).toEqual({ options: [], selected: '', hint: null, canCreate: false });
  });
  it('preselects the default', () => {
    expect(baseChoice(branches())).toEqual({ options: ['main', 'origin/main'], selected: 'origin/main', hint: null, canCreate: true });
  });
  it('blocks creation in an empty repository', () => {
    const c = baseChoice(branches({ branches: [], default: null, automatic: null }));
    expect(c.canCreate).toBe(false);
    expect(c.hint).toBe('This repository has no commits yet.');
  });
  it('warns when the fetch failed but still allows creating', () => {
    const c = baseChoice(branches({ fetch_error: 'offline' }));
    expect(c.canCreate).toBe(true);
    expect(c.hint).toBe("Couldn't fetch origin — branches may be stale.");
  });
  it('offers a detached default that is not a branch', () => {
    const c = baseChoice(branches({ default: 'abc1234', automatic: 'abc1234' }));
    expect(c.options).toEqual(['abc1234', 'main', 'origin/main']);
    expect(c.selected).toBe('abc1234');
  });
});

describe('staleDefault', () => {
  it('is the configured base when it no longer resolves', () => {
    expect(staleDefault(branches({ configured: 'origin/gone' }))).toBe('origin/gone');
  });
  it('is null when the configured base is in use or unset', () => {
    expect(staleDefault(branches({ configured: 'main', default: 'main' }))).toBeNull();
    expect(staleDefault(branches())).toBeNull();
    expect(staleDefault(null)).toBeNull();
  });
});
