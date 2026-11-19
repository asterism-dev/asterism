import { describe, expect, it } from 'vitest';
import { splitHours, toHours } from './archiveSettings';

describe('archive age', () => {
  it('shows whole days as days', () => {
    expect(splitHours(48)).toEqual({ amount: 2, unit: 'days' });
  });

  it('shows other values as hours', () => {
    expect(splitHours(36)).toEqual({ amount: 36, unit: 'hours' });
  });

  it('converts back to hours', () => {
    expect(toHours(2, 'days')).toBe(48);
    expect(toHours(5, 'hours')).toBe(5);
  });
});
