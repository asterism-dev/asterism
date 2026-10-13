import { describe, expect, it } from 'vitest';
import { bindWarning, interfaceLabel, statusLine } from './remoteSettings';
import type { RemoteStatus } from './types';

const config = { enabled: true, bind: '100.1.2.3', port: 7420 };

describe('remote settings', () => {
  it('labels interfaces', () => {
    expect(interfaceLabel({ name: 'utun4', address: '100.1.2.3', tailscale: true })).toBe(
      'Tailscale · 100.1.2.3',
    );
    expect(interfaceLabel({ name: 'en0', address: '192.168.1.2', tailscale: false })).toBe(
      'en0 · 192.168.1.2',
    );
    expect(interfaceLabel({ name: 'all', address: '0.0.0.0', tailscale: false })).toBe(
      'All interfaces · 0.0.0.0',
    );
  });

  it('warns about binds reachable outside the tailnet', () => {
    expect(bindWarning('0.0.0.0')).toMatch(/every network/);
    expect(bindWarning('::')).toMatch(/every network/);
    expect(bindWarning('100.1.2.3')).toBeNull();
    expect(bindWarning('127.0.0.1')).toBeNull();
  });

  it('describes the listener state', () => {
    const s = (patch: Partial<RemoteStatus>): RemoteStatus => ({
      config,
      state: 'disabled',
      ...patch,
    });
    expect(statusLine(s({}))).toBe('Off');
    expect(statusLine(s({ state: 'listening', address: '100.1.2.3:7420' }))).toBe(
      'Listening on 100.1.2.3:7420',
    );
    expect(statusLine(s({ state: 'error', error: 'port in use' }))).toBe(
      'Not listening: port in use',
    );
  });
});
