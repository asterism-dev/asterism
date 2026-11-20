import { expect, it } from 'vitest';
import {
  defaultMachineFor,
  forgetConfirmText,
  localTaskActions,
  machineAgents,
  machineBlocksInput,
  machineStatusLine,
  taskMachine,
  updateConfirmText,
} from './machines';
import type { AgentInfo, Machine, MachineSessionRef, Project, Task } from './types';

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

it('machineBlocksInput only blocks once the machine list is known', () => {
  expect(machineBlocksInput(task(7), [machine('offline')], true)).toBe(true);
  expect(machineBlocksInput(task(7), [machine('ready')], true)).toBe(false);
  expect(machineBlocksInput(task(null), [], true)).toBe(false);
  expect(machineBlocksInput(task(7), [], false)).toBe(false);
});

const agent = (name: string, available = true) => ({ name, available }) as AgentInfo;
const withAgents = (agents?: AgentInfo[]) => {
  const m = machine('ready');
  return { ...m, status: { ...m.status, agents } } as Machine;
};

it('machineAgents returns local agents for Local', () => {
  expect(machineAgents(null, [agent('claude'), agent('x', false)])).toEqual([agent('claude')]);
});

it('machineAgents returns the available agents of a machine', () => {
  const m = withAgents([agent('claude'), agent('codex', false)]);
  expect(machineAgents(m, [agent('local-only')])).toEqual([agent('claude')]);
});

it('machineAgents is empty for a machine without agents', () => {
  expect(machineAgents(withAgents(), [agent('claude')])).toEqual([]);
  expect(machineAgents(withAgents([]), [agent('claude')])).toEqual([]);
});

it('localTaskActions offers local agents and reveal for local tasks', () => {
  const local = [agent('claude')];
  expect(localTaskActions(task(null), [], local)).toEqual({ agents: local, reveal: true });
  expect(localTaskActions(task(undefined), [], local)).toEqual({ agents: local, reveal: true });
});

it('localTaskActions offers the machine agents but never reveal for remote tasks', () => {
  const m = withAgents([agent('codex')]);
  expect(localTaskActions(task(7), [m], [agent('claude')])).toEqual({
    agents: [agent('codex')],
    reveal: false,
  });
  expect(localTaskActions(task(9), [m], [agent('claude')])).toEqual({ agents: [], reveal: false });
});

const ref = (title: string, kind: MachineSessionRef['kind']): MachineSessionRef => ({
  session_id: 1,
  task_id: 1,
  task_title: title,
  kind,
});

it('lists sessions that resume and end', () => {
  const text = updateConfirmText({
    resume: [ref('Fix bug', { type: 'agent', name: 'claude' })],
    end: [ref('Build', { type: 'command', argv: ['make', 'dev'] }), ref('Logs', { type: 'shell' })],
    exact: true,
  });
  expect(text).toBe(
    [
      'Update now? The machine restarts its daemon.',
      'Will resume:',
      '- Fix bug (claude)',
      'Will end:',
      '- Build (make dev)',
      '- Logs (shell)',
    ].join('\n'),
  );
});

it('lists only sessions that end', () => {
  const text = updateConfirmText({
    resume: [],
    end: [ref('Logs', { type: 'shell' })],
    exact: true,
  });
  expect(text).not.toContain('Will resume:');
  expect(text).toContain('Will end:\n- Logs (shell)');
});

it('says there are no running sessions', () => {
  expect(updateConfirmText({ resume: [], end: [], exact: true })).toContain('No running sessions.');
});

it('says sessions may end when the preview is not exact', () => {
  const text = updateConfirmText({
    resume: [],
    end: [ref('Logs', { type: 'shell' })],
    exact: false,
  });
  expect(text).toContain('These sessions may end:\n- Logs (shell)');
  expect(text).not.toContain('Will end:');
});

it('forget lists the tasks to archive', () => {
  const tasks = [{ title: 'A' }, { title: 'B' }] as Task[];
  expect(forgetConfirmText('box', tasks)).toBe(
    'Forget box? Its tasks will be archived here and anything left on the machine is abandoned:\n- A\n- B',
  );
});

it('forget without tasks', () => {
  expect(forgetConfirmText('box', [])).toBe(
    'Forget box? Its tasks will be archived here and anything left on the machine is abandoned:\nIt has no active tasks.',
  );
});

const projectWith = (default_machine_id?: number | null) =>
  ({ id: 1, default_machine_id }) as Project;

it('defaults to Local without a project default', () => {
  expect(defaultMachineFor(undefined, [machine('ready')])).toBe(0);
  expect(defaultMachineFor(projectWith(), [machine('ready')])).toBe(0);
  expect(defaultMachineFor(projectWith(null), [machine('ready')])).toBe(0);
});

it('uses the project default machine when it is ready', () => {
  expect(defaultMachineFor(projectWith(7), [machine('ready')])).toBe(7);
});

it.each(['connecting', 'offline', 'blocked'] as const)(
  'falls back to Local when the default machine is %s',
  (st) => {
    expect(defaultMachineFor(projectWith(7), [machine(st)])).toBe(0);
  },
);

it('falls back to Local when the default machine no longer exists', () => {
  expect(defaultMachineFor(projectWith(9), [machine('ready')])).toBe(0);
});
