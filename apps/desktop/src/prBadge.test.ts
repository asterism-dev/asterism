import { describe, expect, it } from 'vitest';
import { prBadge, prSummary } from './prBadge';
import type { PullRequest } from './types';

const pr = (over: Partial<PullRequest> = {}): PullRequest => ({
  number: 7, url: 'https://x/7', title: 'Fix', state: 'open', review: 'none',
  checks: { state: 'success', failing: [] }, ...over,
});

describe('prBadge', () => {
  it('picks the tone by state, then checks and review', () => {
    expect(prBadge(pr({ state: 'merged', checks: { state: 'failure', failing: ['x'] } })).tone).toBe('merged');
    expect(prBadge(pr({ state: 'closed' })).tone).toBe('closed');
    expect(prBadge(pr({ state: 'draft' })).tone).toBe('draft');
    expect(prBadge(pr({ checks: { state: 'failure', failing: ['lint'] } })).tone).toBe('failure');
    expect(prBadge(pr({ review: 'changes_requested' })).tone).toBe('failure');
    expect(prBadge(pr({ checks: { state: 'pending', failing: [] } })).tone).toBe('pending');
    expect(prBadge(pr({ checks: { state: 'none', failing: [] } })).tone).toBe('success');
  });

  it('labels with the number and explains in the tooltip', () => {
    const badge = prBadge(pr({ review: 'approved', checks: { state: 'failure', failing: ['lint', 'test'] } }));
    expect(badge.label).toBe('#7');
    expect(badge.tooltip).toBe('Open · approved · 2 failing: lint, test');
    expect(prSummary(pr({ review: 'review_required', checks: { state: 'pending', failing: [] } }))).toBe('review required · checks running');
    expect(prSummary(pr({ checks: { state: 'none', failing: [] } }))).toBe('no checks');
  });
});
