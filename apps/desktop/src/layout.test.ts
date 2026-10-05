import { describe, expect, it } from 'vitest';
import { LEFT, RIGHT, clampWidth, layout, parseLayout, toggleRightPanel } from './layout';

describe('layout', () => {
  it('falls back to defaults and clamps stored widths', () => {
    expect(parseLayout(null)).toEqual({ leftWidth: LEFT.initial, leftOpen: true, rightWidth: RIGHT.initial, rightOpen: false, rightPanel: 'diff' });
    expect(parseLayout('not json').leftWidth).toBe(LEFT.initial);
    const parsed = parseLayout(JSON.stringify({ leftWidth: 5000, rightWidth: 10, leftOpen: false, rightPanel: 'activity' }));
    expect(parsed).toMatchObject({ leftWidth: LEFT.max, rightWidth: RIGHT.min, leftOpen: false, rightPanel: 'activity' });
    expect(clampWidth(300.6, LEFT)).toBe(301);
  });

  it('toggles the right column per panel', () => {
    layout.rightOpen = false;
    toggleRightPanel('activity');
    expect(layout).toMatchObject({ rightOpen: true, rightPanel: 'activity' });
    toggleRightPanel('diff');
    expect(layout).toMatchObject({ rightOpen: true, rightPanel: 'diff' });
    toggleRightPanel('diff');
    expect(layout.rightOpen).toBe(false);
  });
});
