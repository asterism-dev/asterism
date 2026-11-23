import type { SettingsDraft } from './pluginsView';
import type {
  AgentInfo,
  Machine,
  MachineProviderInfo,
  MachineSessionRef,
  PluginInfo,
  MachineUpdatePreview,
  Project,
  SessionKind,
  SettingSpec,
  SettingValue,
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
    case 'hibernated':
      return 'Hibernated';
  }
}

export const LOCAL_MACHINE = 0;

export function machineOptions(machines: Machine[], disableUnready = false) {
  return [
    { value: LOCAL_MACHINE, label: 'Local' },
    ...machines.map((m) => ({
      value: m.id,
      label: m.status.state === 'ready' ? m.name : `${m.name} — ${machineStatusLine(m)}`,
      disabled: disableUnready && m.status.state !== 'ready' && m.status.state !== 'hibernated',
    })),
  ];
}

export function defaultMachineFor(project: Project | undefined, machines: Machine[]): number {
  const id = project?.default_machine_id;
  const ready = machines.some((m) => m.id === id && m.status.state === 'ready');
  return ready && id != null ? id : LOCAL_MACHINE;
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

export function machineTaskList(m: Machine, tasks: Task[]): Task[] {
  return tasks.filter((t) => t.machine_id === m.id && !t.archived);
}

export function machineSummary(m: Machine, tasks: Task[]): string {
  const count = machineTaskList(m, tasks).length;
  return [
    ...machineAgents(m, []).map((a) => a.display_name || a.name),
    count > 0 && `${count} active ${count === 1 ? 'task' : 'tasks'}`,
  ]
    .filter(Boolean)
    .join(' · ');
}

export function providerGroups(infos: MachineProviderInfo[]) {
  return infos.map((info) => ({
    provider: info.provider,
    label: info.display_name,
    error: info.error,
    options: [
      ...info.vms.map((vm) => ({
        value: `provider:${info.provider}:vm:${vm.ref}`,
        label: vm.state === 'hibernated' ? `${vm.name} (hibernated)` : vm.name,
      })),
      { value: `provider:${info.provider}:new`, label: '+ New VM…' },
    ],
  }));
}

export function parseProviderValue(
  value: string | number,
): { provider: string; ref: string | null } | null {
  if (typeof value !== 'string') return null;
  const m = /^provider:([^:]+):(?:new|vm:(.+))$/s.exec(value);
  return m ? { provider: m[1], ref: m[2] ?? null } : null;
}

export function suggestVmName(title: string, taken: string[]): string {
  const slug =
    title
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+/, '')
      .slice(0, 40)
      .replace(/-+$/, '') || 'task';
  let name = slug;
  for (let n = 2; taken.includes(name); n++) name = `${slug}-${n}`;
  return name;
}

export function vmOptionsDraft(specs: SettingSpec[]): SettingsDraft {
  const values: Record<string, SettingValue> = {};
  for (const spec of specs) {
    const d = spec.default;
    values[spec.key] =
      typeof d === 'string' || typeof d === 'number' || typeof d === 'boolean'
        ? d
        : spec.type === 'bool'
          ? false
          : '';
  }
  return { values, secrets: {}, cleared: [] };
}

/** Empty fields are left out so the provider applies its own default. */
export function vmOptionsPayload(draft: SettingsDraft): Record<string, SettingValue> {
  return Object.fromEntries(Object.entries(draft.values).filter(([, v]) => v !== ''));
}

export function builtinAgents(agents: AgentInfo[], plugins: PluginInfo[]): AgentInfo[] {
  const builtin = new Set(plugins.filter((p) => p.origin === 'builtin').map((p) => p.name));
  return agents.filter((a) => a.available && builtin.has(a.plugin));
}

export function providerLabel(plugins: PluginInfo[], id: string): string {
  const cap = plugins
    .flatMap((p) => p.capabilities)
    .find((c) => c.kind === 'machine_provider' && c.id === id);
  return cap?.description || id;
}
