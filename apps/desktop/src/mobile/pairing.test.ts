import { describe, expect, it, vi } from 'vitest';
import {
  adoptPairing,
  clearToken,
  loadToken,
  socketUrl,
  tokenFromHash,
  TOKEN_KEY,
} from './pairing';

function memory(initial: Record<string, string> = {}) {
  const data = { ...initial };
  return {
    data,
    getItem: (k: string) => data[k] ?? null,
    setItem: (k: string, v: string) => void (data[k] = v),
    removeItem: (k: string) => void delete data[k],
  };
}

describe('pairing', () => {
  it('reads a token only from a pairing hash', () => {
    expect(tokenFromHash('#token=ab_C-9')).toBe('ab_C-9');
    expect(tokenFromHash('#/t/1')).toBeNull();
    expect(tokenFromHash('#token=')).toBeNull();
    expect(tokenFromHash('#token=a b')).toBeNull();
  });

  it('stores a fresh token and removes it from the address bar', () => {
    const storage = memory();
    const replaceState = vi.fn();
    const token = adoptPairing(
      { hash: '#token=abc', pathname: '/', search: '' },
      { replaceState },
      storage,
    );
    expect(token).toBe('abc');
    expect(storage.data[TOKEN_KEY]).toBe('abc');
    expect(replaceState).toHaveBeenCalledWith(null, '', '/#/');
  });

  it('falls back to the stored token', () => {
    const storage = memory({ [TOKEN_KEY]: 'old' });
    const replaceState = vi.fn();
    expect(
      adoptPairing({ hash: '#/t/2', pathname: '/', search: '' }, { replaceState }, storage),
    ).toBe('old');
    expect(replaceState).not.toHaveBeenCalled();
  });

  it('survives unavailable storage', () => {
    const broken = {
      getItem: () => {
        throw new Error('blocked');
      },
      removeItem: () => {
        throw new Error('blocked');
      },
    };
    expect(loadToken(broken)).toBeNull();
    expect(() => clearToken(broken)).not.toThrow();
  });

  it('derives the socket url from the page', () => {
    expect(socketUrl({ protocol: 'http:', host: '100.1.2.3:7420' })).toBe('ws://100.1.2.3:7420/ws');
    expect(socketUrl({ protocol: 'https:', host: 'mac.ts.net' })).toBe('wss://mac.ts.net/ws');
  });
});
