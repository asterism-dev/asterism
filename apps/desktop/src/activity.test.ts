import { describe, expect, it } from 'vitest';
import { activityFor, anyRunning, formatDiffCount, upsertActivity } from './activity';
import type { Activity } from './types';

const act = (id: number, over: Partial<Activity> = {}): Activity => ({
  id,
  kind: 'clone',
  title: `a${id}`,
  token: null,
  project_id: null,
  task_id: null,
  started_at: id,
  finished_at: null,
  status: 'running',
  steps: [],
  ...over,
});

describe('upsertActivity', () => {
  it('replaces in place, keeping position', () => {
    const out = upsertActivity([act(2), act(1)], act(1, { status: 'done' }));
    expect(out.map((a) => a.id)).toEqual([2, 1]);
    expect(out[1].status).toBe('done');
  });
  it('prepends new activities', () => {
    expect(upsertActivity([act(1)], act(2)).map((a) => a.id)).toEqual([2, 1]);
  });
  it('caps at 50, dropping the oldest', () => {
    const list = Array.from({ length: 50 }, (_, i) => act(50 - i));
    const out = upsertActivity(list, act(51));
    expect(out).toHaveLength(50);
    expect(out[0].id).toBe(51);
    expect(out.some((a) => a.id === 1)).toBe(false);
  });
});

describe('formatDiffCount', () => {
  it('formats counts', () => {
    expect(formatDiffCount(0)).toBe('');
    expect(formatDiffCount(999)).toBe('+999');
    expect(formatDiffCount(1200)).toBe('+1.2k');
    expect(formatDiffCount(4000)).toBe('+4k');
  });
});

describe('anyRunning', () => {
  it('detects running activities', () => {
    expect(anyRunning([act(1, { status: 'done' })])).toBe(false);
    expect(anyRunning([act(1, { status: 'done' }), act(2)])).toBe(true);
  });
});

describe('activityFor', () => {
  it('matches by token', () => {
    const list = [act(2, { token: 'b' }), act(1, { token: 'a' })];
    expect(activityFor(list, 'a')?.id).toBe(1);
  });
  it('is undefined when absent', () => {
    expect(activityFor([act(1)], 'a')).toBeUndefined();
  });
});
