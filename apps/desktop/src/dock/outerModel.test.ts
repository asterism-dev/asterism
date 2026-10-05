import { describe, expect, it } from 'vitest';
import { keptSizes, missingPanes, parseOuter, toggleAction } from './outerModel';

describe('outer layout model', () => {
  it('toggles closed → open, background → activate, front → close', () => {
    expect(toggleAction('closed')).toBe('open');
    expect(toggleAction('background')).toBe('activate');
    expect(toggleAction('front')).toBe('close');
  });

  it('rejects anything that is not a stored layout', () => {
    expect(parseOuter(null)).toBeNull();
    expect(parseOuter('not json')).toBeNull();
    expect(parseOuter('{"leftWidth":260}')).toBeNull();
    expect(parseOuter('{"layout":{"panels":{}}}')).toBeNull();
  });

  it('keeps the layout and only known closable panes', () => {
    const stored = parseOuter(JSON.stringify({ layout: { grid: {}, panels: {} }, closed: ['diff', 'workspace', 'bogus', 'activity'] }));
    expect(stored).toEqual({ layout: { grid: {}, panels: {} }, closed: ['diff', 'activity'] });
    expect(parseOuter('{"layout":{"grid":{}}}')?.closed).toEqual([]);
  });

  it('adds absent panes unless closed on purpose, and always the workspace', () => {
    expect(missingPanes([], [])).toEqual(['workspace', 'projects', 'diff', 'activity']);
    expect(missingPanes(['projects', 'diff'], ['activity'])).toEqual(['workspace']);
    expect(missingPanes(['projects'], ['workspace' as never])).toEqual(['workspace', 'diff', 'activity']);
  });

  it('keeps the sizes of surviving groups except the one that absorbs the space', () => {
    const before = [{ id: 'a', width: 260, height: 800 }, { id: 'w', width: 700, height: 800 }, { id: 'd', width: 420, height: 800 }];
    expect(keptSizes(before, ['a', 'w'], 'w')).toEqual([{ id: 'a', width: 260, height: 800 }]);
    expect(keptSizes(before, ['a', 'w'], undefined)).toEqual([{ id: 'a', width: 260, height: 800 }, { id: 'w', width: 700, height: 800 }]);
  });
});
