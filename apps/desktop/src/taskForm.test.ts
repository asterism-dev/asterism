import { describe, expect, it } from 'vitest';
import {
  createRequest, filterBranches, defaultBranch, finalSlug, issueStateClass, liveSlug, prDisabledReason,
  randomSlug, randomSuffix, type TaskForm,
} from './taskForm';
import type { PrHit } from './types';

const form = (over: Partial<TaskForm> = {}): TaskForm => ({
  projectId: 1, title: 'fix-login', placeholder: 'two-hairs-begin', prompt: ' do it ', agent: 'claude',
  mode: 'new', base: 'origin/main', branch: 'asterism/fix-login-x8d4t', checkout: '', push: true, issue: null, ...over,
});
const pr = (over: Partial<PrHit> = {}): PrHit => ({
  number: 1, title: 'T', url: 'u', author: 'a', head_branch: 'feature/x', draft: false, from_fork: false, ...over,
});

describe('taskForm', () => {
  it('slugifies while typing and keeps one trailing dash until submit', () => {
    expect(liveSlug('Fix Login ')).toBe('fix-login-');
    expect(liveSlug('  --Fix__the  LOGIN!!')).toBe('fix-the-login-');
    expect(finalSlug('Fix Login ')).toBe('fix-login');
    expect(finalSlug('!!!')).toBe('');
    expect(finalSlug('Ärger')).toBe('rger');
  });

  it('builds random names from the injected generator', () => {
    expect(randomSlug(() => 0)).toMatch(/^[a-z]+-[a-z]+-[a-z]+$/);
    expect(randomSuffix(() => 0)).toBe('aaaaa');
    expect(randomSuffix(() => 0.999)).toMatch(/^[a-z0-9]{5}$/);
  });

  it('derives the default branch name', () => {
    expect(defaultBranch('fix-login', 'x8d4t')).toBe('asterism/fix-login-x8d4t');
  });

  it('filters branches by a case-insensitive substring', () => {
    const list = ['main', 'origin/feature/TRA-12-login', 'origin/develop'];
    expect(filterBranches(list, '  ')).toEqual(list);
    expect(filterBranches(list, 'tra-12')).toEqual(['origin/feature/TRA-12-login']);
    expect(filterBranches(list, 'xyz')).toEqual([]);
  });

  it('explains why a PR cannot be picked', () => {
    expect(prDisabledReason(pr())).toBeNull();
    expect(prDisabledReason(pr({ from_fork: true }))).toBe('PR from a fork — not supported yet');
  });

  it('maps issue states to a colour class', () => {
    expect(issueStateClass('open')).toBe('open');
    expect(issueStateClass('In Progress')).toBe('progress');
    expect(issueStateClass('Done')).toBe('done');
    expect(issueStateClass('closed')).toBe('done');
    expect(issueStateClass('Canceled')).toBe('canceled');
  });

  it('creates a new branch with push', () => {
    expect(createRequest(form())).toEqual({
      project_id: 1, title: 'fix-login', prompt: 'do it', agent: 'claude', base: 'origin/main', issue: null,
      branch: 'asterism/fix-login-x8d4t', checkout: null, push: true,
    });
  });

  it('checks out an existing branch without base, branch or push', () => {
    expect(createRequest(form({ mode: 'checkout', checkout: 'feature/x' }))).toMatchObject({
      base: null, branch: null, checkout: 'feature/x', push: false,
    });
  });

  it('falls back to the placeholder title and drops the prompt without an agent', () => {
    expect(createRequest(form({ title: '!!!', agent: '' }))).toMatchObject({ title: 'two-hairs-begin', agent: null, prompt: null });
  });
});
