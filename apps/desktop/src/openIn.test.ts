import { afterEach, describe, expect, it, vi } from 'vitest';
import { lastApp, rememberApp } from './openIn';

const available = ['Finder', 'Zed'];

function stubStorage(stored: string | null) {
  vi.stubGlobal('localStorage', {
    getItem: () => stored,
    setItem: vi.fn(),
  });
}

afterEach(() => vi.unstubAllGlobals());

describe('lastApp', () => {
  it('defaults to Finder', () => {
    stubStorage(null);
    expect(lastApp(available)).toBe('Finder');
  });

  it('returns a stored available app', () => {
    stubStorage('Zed');
    expect(lastApp(available)).toBe('Zed');
  });

  it('ignores a stored app that is not available', () => {
    stubStorage('Zed');
    expect(lastApp(['Finder'])).toBe('Finder');
  });

  it('falls back when storage throws', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
    });
    expect(lastApp(available)).toBe('Finder');
  });
});

describe('rememberApp', () => {
  it('swallows storage errors', () => {
    vi.stubGlobal('localStorage', {
      setItem: () => {
        throw new Error('denied');
      },
    });
    expect(() => rememberApp('Zed')).not.toThrow();
  });
});
