import { describe, expect, it } from 'vitest';
import { formatRoute, parseRoute, type Route } from './route';

describe('route', () => {
  it('parses task routes', () => {
    expect(parseRoute('#/t/4')).toEqual({
      name: 'task',
      taskId: 4,
      tab: 'sessions',
      sessionId: null,
    });
    expect(parseRoute('#/t/4/terminal/9')).toEqual({
      name: 'task',
      taskId: 4,
      tab: 'terminal',
      sessionId: 9,
    });
  });

  it('falls back to the list for anything else', () => {
    for (const hash of ['', '#', '#/', '#/t/x', '#token=abc', '#/nope/1'])
      expect(parseRoute(hash)).toEqual({ name: 'list' });
  });

  it('round-trips', () => {
    const routes: Route[] = [
      { name: 'list' },
      { name: 'task', taskId: 1, tab: 'sessions', sessionId: null },
      { name: 'task', taskId: 1, tab: 'terminal', sessionId: 3 },
    ];
    for (const r of routes) expect(parseRoute(formatRoute(r))).toEqual(r);
  });
});
