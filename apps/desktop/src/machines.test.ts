import { expect, it } from 'vitest';
import { machineStatusLine, taskMachine } from './machines';
import type { Machine, Task } from './types';

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

const machine = (state: Machine['status']['state']) =>
  ({
    id: 7,
    name: 'vm02',
    destination: 'vm02',
    created_at: 0,
    status: { state, platform: null, error: null, update_available: false },
  }) as Machine;
const task = (machine_id: number | null | undefined) => ({ id: 1, machine_id }) as Task;

it('taskMachine is null for local tasks, including old daemons without the field', () => {
  expect(taskMachine(task(null), [machine('ready')])).toBeNull();
  expect(taskMachine(task(undefined), [machine('ready')])).toBeNull();
});

it('taskMachine reports a ready machine', () => {
  expect(taskMachine(task(7), [machine('ready')])).toEqual({ name: 'vm02', ready: true });
});

it.each(['offline', 'blocked', 'connecting'] as const)('taskMachine is not ready when %s', (st) => {
  expect(taskMachine(task(7), [machine(st)])).toEqual({ name: 'vm02', ready: false });
});

it('taskMachine names an unknown machine by id and treats it as not ready', () => {
  expect(taskMachine(task(9), [machine('ready')])).toEqual({ name: 'machine 9', ready: false });
});
