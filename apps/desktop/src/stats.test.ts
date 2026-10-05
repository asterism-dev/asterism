import { describe, expect, it } from 'vitest';
import { formatBytes, formatCpu } from './stats';

describe('stats formatting', () => {
  it('shows megabytes below one gigabyte', () => {
    expect(formatBytes(31 * 1024 * 1024)).toBe('31 MB');
    expect(formatBytes(1.25 * 1024 ** 3)).toBe('1.3 GB');
  });

  it('keeps one decimal for small CPU values', () => {
    expect(formatCpu(0)).toBe('0.0 %');
    expect(formatCpu(3.46)).toBe('3.5 %');
    expect(formatCpu(142.4)).toBe('142 %');
  });
});
