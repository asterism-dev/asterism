import type {
  AgentInfo,
  Machine,
  MachineSessionRef,
  MachineUpdatePreview,
  SessionKind,
  Task,
} from './types';

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

export function machineAgents(machine: Machine | null, localAgents: AgentInfo[]): AgentInfo[] {
  const agents = machine ? (machine.status.agents ?? []) : localAgents;
  return agents.filter((a) => a.available);
}

/** Finder only works on this Mac; agents come from the task's machine. */
export function localTaskActions(
  task: Task,
  machines: Machine[],
  localAgents: AgentInfo[],
): { agents: AgentInfo[]; reveal: boolean } {
  const machine = machines.find((m) => m.id === task.machine_id) ?? null;
  const remote = task.machine_id != null;
  return {
    agents: remote ? machineAgents(machine, []) : machineAgents(null, localAgents),
    reveal: !remote,
  };
}

export function canUpdateMachine(m: Machine): boolean {
  const { state, update_available, error } = m.status;
  return update_available || (state === 'blocked' && !!error?.includes('incompatible version'));
}

function sessionKindLabel(kind: SessionKind): string {
  switch (kind.type) {
    case 'agent':
      return kind.name;
    case 'shell':
      return 'shell';
    case 'command':
      return kind.argv.join(' ');
  }
}

const sessionLines = (refs: MachineSessionRef[]) =>
  refs.map((r) => `- ${r.task_title} (${sessionKindLabel(r.kind)})`);

export function updateConfirmText(p: MachineUpdatePreview): string {
  const lines = ['Update now? The machine restarts its daemon.'];
  if (!p.resume.length && !p.end.length) lines.push('No running sessions.');
  if (p.resume.length) lines.push('Will resume:', ...sessionLines(p.resume));
  if (p.end.length) {
    lines.push(p.exact ? 'Will end:' : 'These sessions may end:', ...sessionLines(p.end));
  }
  return lines.join('\n');
}

export function forgetConfirmText(name: string, tasks: Task[]): string {
  const head = `Forget ${name}? Its tasks will be archived here and anything left on the machine is abandoned:`;
  return [
    head,
    ...(tasks.length ? tasks.map((t) => `- ${t.title}`) : ['It has no active tasks.']),
  ].join('\n');
}
