import type { Machine, Task } from './types';

export function machineStatusLine(m: Machine): string {
  const { state, platform, error, update_available } = m.status;
  switch (state) {
    case 'ready':
      return ['Connected', platform, update_available && 'update available']
        .filter(Boolean)
        .join(' · ');
    case 'connecting':
      return 'Connecting…';
    case 'offline':
      return `Offline, retrying: ${error ?? 'unknown error'}`;
    case 'blocked':
      return `Needs attention: ${error ?? 'unknown error'}`;
  }
}

export function taskMachine(
  task: Task,
  machines: Machine[],
): { name: string; ready: boolean } | null {
  if (task.machine_id == null) return null;
  const m = machines.find((x) => x.id === task.machine_id);
  return { name: m?.name ?? `machine ${task.machine_id}`, ready: m?.status.state === 'ready' };
}

export function machineBlocksInput(task: Task, machines: Machine[], loaded: boolean): boolean {
  return loaded && taskMachine(task, machines)?.ready === false;
}
