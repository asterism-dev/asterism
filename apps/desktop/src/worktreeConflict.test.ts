import { describe, expect, it } from 'vitest';
import { detectConflict, keepChoice, reloadsOnError, replaceBlocked } from './worktreeConflict';
import type { ProjectBranches } from './types';

const branches = (over: Partial<ProjectBranches> = {}): ProjectBranches => ({
  branches: [],
  default: 'main',
  automatic: 'main',
  configured: null,
  fetch_error: null,
  worktree_root: '/wt/repo',
  local: ['main', 'feature/x', 'feature/x-2', 'only-branch'],
  remote: [],
  worktrees: [
    { dir: 'feature/x', branch: 'feature/x', tasks: [{ id: 3, title: 'a', archived: true }] },
    { dir: 'loose', branch: 'loose', tasks: [] },
  ],
  ...over,
});

describe('detectConflict', () => {
  it('finds nothing for a free branch or before branches load', () => {
    expect(detectConflict(branches(), 'new', 'feature/new', '')).toBeNull();
    expect(detectConflict(null, 'new', 'feature/x', '')).toBeNull();
    expect(detectConflict(branches(), 'new', '  ', '')).toBeNull();
  });

  it('offers all choices for an existing worktree and previews the next free suffix', () => {
    const c = detectConflict(branches(), 'new', ' feature/x ', '')!;
    expect(c.name).toBe('feature/x');
    expect(c.worktree?.tasks.map((t) => t.id)).toEqual([3]);
    expect(c.choices).toEqual(['reuse', 'suffix', 'replace']);
    expect(c.suffixed).toBe('feature/x-3');
    expect(replaceBlocked(c)).toBe(true);
  });

  it('treats an existing branch without worktree as a conflict only when creating', () => {
    const c = detectConflict(branches(), 'new', 'only-branch', '')!;
    expect(c.worktree).toBeNull();
    expect(replaceBlocked(c)).toBe(false);
    expect(detectConflict(branches(), 'checkout', '', 'only-branch')).toBeNull();
  });

  it('offers reuse and replace for a checkout whose worktree exists', () => {
    const c = detectConflict(branches(), 'checkout', '', 'loose')!;
    expect(c.choices).toEqual(['reuse', 'replace']);
    expect(c.suffixed).toBeNull();
    expect(replaceBlocked(c)).toBe(false);
  });
});

describe('keepChoice', () => {
  it('falls back to reuse when the choice is not offered any more', () => {
    const checkout = detectConflict(branches(), 'checkout', '', 'loose')!;
    expect(keepChoice(checkout, 'suffix')).toBe('reuse');
    expect(keepChoice(checkout, 'replace')).toBe('replace');
  });
});

describe('reloadsOnError', () => {
  it('only treats branch_exists as a stale conflict when no choice was sent', () => {
    expect(reloadsOnError('branch_exists', null)).toBe(true);
    expect(reloadsOnError('branch_exists', 'suffix')).toBe(false);
    expect(reloadsOnError('git', null)).toBe(false);
  });
});
