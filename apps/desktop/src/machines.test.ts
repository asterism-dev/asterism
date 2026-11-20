import { expect, it } from 'vitest';
import { machineStatusLine } from './machines';
import type { Machine } from './types';

it.each([
  [
    { state: 'ready', platform: 'linux-x86_64', error: null, update_available: false },
    'Connected · linux-x86_64',
  ],
  [
    { state: 'ready', platform: 'linux-x86_64', error: null, update_available: true },
    'Connected · linux-x86_64 · update available',
  ],
  [{ state: 'connecting', platform: null, error: null, update_available: false }, 'Connecting…'],
  [
    {
      state: 'offline',
      platform: null,
      error: 'ssh: Could not resolve hostname vm02',
      update_available: false,
    },
    'Offline, retrying: ssh: Could not resolve hostname vm02',
  ],
  [
    {
      state: 'blocked',
      platform: null,
      error: 'Permission denied (publickey).',
      update_available: false,
    },
    'Needs attention: Permission denied (publickey).',
  ],
])('status line %#', (status, line) => {
  expect(
    machineStatusLine({ id: 1, name: 'vm', destination: 'vm', created_at: 0, status } as Machine),
  ).toBe(line);
});
