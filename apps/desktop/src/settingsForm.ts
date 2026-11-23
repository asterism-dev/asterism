import type { AgentConfig, AgentInfo, EnvSettings } from './types';

export interface EnvRow {
  key: string;
  value: string;
}
export interface AgentForm {
  args: string[];
  set: EnvRow[];
  remove: string[];
  machineSet: EnvRow[];
  machineRemove: string[];
  machineFromLocal: boolean;
  mcpText: string;
  hooksText: string;
  hibernateAfter: string;
}
export interface FormErrors {
  env?: string;
  machineEnv?: string;
  mcp?: string;
  hooks?: string;
  hibernate?: string;
  save?: string;
}

export const BASE_AGENTS = ['shell', 'command'];
export const MCP_EXAMPLE =
  '{\n  "mcpServers": {\n    "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "."] }\n  }\n}';
export const HOOKS_EXAMPLE =
  '{\n  "hooks": {\n    "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }]\n  }\n}';

type Parsed = { value: Record<string, unknown> | null } | { error: string };

export interface AgentSections {
  args: boolean;
  mcp: boolean;
  hooks: boolean;
  hibernate: boolean;
}

export function agentSections(info: AgentInfo | undefined): AgentSections {
  const supported = info?.settings ?? [];
  return {
    args: supported.includes('args'),
    mcp: supported.includes('mcp'),
    hooks: supported.includes('hooks'),
    hibernate: supported.includes('hibernate'),
  };
}

export function emptyForm(): AgentForm {
  return {
    args: [],
    set: [],
    remove: [],
    machineSet: [],
    machineRemove: [],
    machineFromLocal: false,
    mcpText: '',
    hooksText: '',
    hibernateAfter: '',
  };
}

function pretty(value: Record<string, unknown> | null): string {
  return value === null ? '' : JSON.stringify(value, null, 2);
}

export function toForm(config: AgentConfig): AgentForm {
  return {
    args: [...config.args],
    set: Object.entries(config.env.set).map(([key, value]) => ({ key, value })),
    remove: [...config.env.remove],
    machineSet: Object.entries(config.machine_env?.set ?? {}).map(([key, value]) => ({
      key,
      value,
    })),
    machineRemove: [...(config.machine_env?.remove ?? [])],
    machineFromLocal: config.machine_env_from_local ?? false,
    mcpText: pretty(config.mcp),
    hooksText: pretty(config.hooks),
    hibernateAfter: config.hibernate_after_min == null ? '' : String(config.hibernate_after_min),
  };
}

export function copyLocalEnv(form: AgentForm): void {
  form.machineSet = form.set.map((row) => ({ ...row }));
  form.machineRemove = [...form.remove];
}

function envSettings(rows: EnvRow[], remove: string[]): { env: EnvSettings; duplicate?: string } {
  // No prototype, so names like "constructor" or "__proto__" are plain keys.
  const set: Record<string, string> = Object.create(null);
  let duplicate: string | undefined;
  for (const row of rows) {
    const key = row.key.trim();
    if (!key) continue;
    if (key in set) duplicate = key;
    set[key] = row.value;
  }
  return { env: { remove: remove.map((r) => r.trim()).filter(Boolean), set }, duplicate };
}

function parseObject(text: string, label: string): Parsed {
  if (!text.trim()) return { value: null };
  try {
    const value: unknown = JSON.parse(text);
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      return { value: value as Record<string, unknown> };
    }
    return { error: `${label} must be a JSON object` };
  } catch (e) {
    return { error: `${label}: ${e instanceof Error ? e.message : String(e)}` };
  }
}

/** Browser-side checks only; the daemon validates the shapes and is authoritative. */
export function fromForm(
  form: AgentForm,
  sections: AgentSections,
): { config: AgentConfig } | { errors: FormErrors } {
  const errors: FormErrors = {};
  const local = envSettings(form.set, form.remove);
  const machine = envSettings(form.machineSet, form.machineRemove);
  if (local.duplicate) errors.env = `${local.duplicate} is set twice`;
  if (machine.duplicate) errors.machineEnv = `${machine.duplicate} is set twice`;
  const mcp: Parsed = sections.mcp ? parseObject(form.mcpText, 'MCP servers') : { value: null };
  const hooks: Parsed = sections.hooks ? parseObject(form.hooksText, 'Hooks') : { value: null };
  const minutes = form.hibernateAfter.trim();
  let hibernate: number | null = null;
  if (sections.hibernate && minutes !== '') {
    if (/^\d+$/.test(minutes)) hibernate = Number(minutes);
    else errors.hibernate = 'Minutes must be a whole number';
  }
  if ('error' in mcp) errors.mcp = mcp.error;
  if ('error' in hooks) errors.hooks = hooks.error;
  if ('error' in mcp || 'error' in hooks || Object.keys(errors).length) return { errors };
  return {
    config: {
      args: sections.args ? form.args.filter((arg) => arg.trim() !== '') : [],
      env: local.env,
      machine_env: machine.env,
      machine_env_from_local: form.machineFromLocal,
      mcp: mcp.value,
      hooks: hooks.value,
      hibernate_after_min: hibernate,
    },
  };
}

function quote(arg: string): string {
  return /^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, "'\\''")}'`;
}

export function commandPreview(agent: string, form: AgentForm): string {
  return [agent, '…', ...form.args.filter((arg) => arg !== '').map(quote)].join(' ');
}
