import type { AgentConfig, AgentInfo } from './types';

export interface EnvRow {
  key: string;
  value: string;
}
export interface AgentForm {
  args: string[];
  set: EnvRow[];
  remove: string[];
  mcpText: string;
  hooksText: string;
  hibernateAfter: string;
}
export interface FormErrors {
  env?: string;
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
  return { args: [], set: [], remove: [], mcpText: '', hooksText: '', hibernateAfter: '' };
}

function pretty(value: Record<string, unknown> | null): string {
  return value === null ? '' : JSON.stringify(value, null, 2);
}

export function toForm(config: AgentConfig): AgentForm {
  return {
    args: [...config.args],
    set: Object.entries(config.env.set).map(([key, value]) => ({ key, value })),
    remove: [...config.env.remove],
    mcpText: pretty(config.mcp),
    hooksText: pretty(config.hooks),
    hibernateAfter: config.hibernate_after_min == null ? '' : String(config.hibernate_after_min),
  };
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
  // No prototype, so names like "constructor" or "__proto__" are plain keys.
  const set: Record<string, string> = Object.create(null);
  for (const row of form.set) {
    const key = row.key.trim();
    if (!key) continue;
    if (key in set) errors.env = `${key} is set twice`;
    set[key] = row.value;
  }
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
      env: { remove: form.remove.map((r) => r.trim()).filter(Boolean), set },
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
