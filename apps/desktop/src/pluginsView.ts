import type {
  AgentInfo,
  CapabilityKind,
  PluginInfo,
  PluginSettings,
  PluginState,
  SearchHit,
  SettingSpec,
  SettingValue,
} from './types';

export function stateLabel(state: PluginState): string | null {
  switch (state.state) {
    case 'ok':
      return null;
    case 'needs_setup':
      return 'needs setup';
    case 'disabled':
      return 'disabled';
    case 'broken':
    case 'failing':
      return state.state;
  }
}

export function stateDetail(state: PluginState): string | null {
  switch (state.state) {
    case 'ok':
    case 'disabled':
      return null;
    case 'needs_setup':
      return `Missing: ${state.missing.join(', ')}`;
    case 'broken':
    case 'failing':
      return state.reason;
  }
}

const KIND_LABELS: Record<CapabilityKind, string> = {
  forge: 'Forge',
  agent: 'Agent',
  command: 'Command',
  task_source: 'Task source',
  panel: 'Panel',
};

export function capabilityChips(plugin: PluginInfo): string[] {
  return plugin.capabilities.map((c) => `${KIND_LABELS[c.kind]}: ${c.id}`);
}

export interface SettingsDraft {
  values: Record<string, SettingValue>;
  secrets: Record<string, string>;
  cleared: string[];
}

function shown(spec: SettingSpec, settings: PluginSettings): SettingValue {
  return settings.values[spec.key] ?? (spec.type === 'bool' ? false : '');
}

export function draftFrom(settings: PluginSettings): SettingsDraft {
  const values: Record<string, SettingValue> = {};
  for (const spec of settings.schema)
    if (spec.type !== 'secret') values[spec.key] = shown(spec, settings);
  return { values, secrets: {}, cleared: [] };
}

/** Only changes: empty text restores the default; secrets are sent only when typed or cleared. */
export function settingsPatch(
  settings: PluginSettings,
  draft: SettingsDraft,
): Record<string, SettingValue | null> {
  const patch: Record<string, SettingValue | null> = {};
  for (const spec of settings.schema) {
    if (spec.type === 'secret') {
      if (draft.secrets[spec.key]) patch[spec.key] = draft.secrets[spec.key];
      else if (draft.cleared.includes(spec.key)) patch[spec.key] = null;
      continue;
    }
    const value = draft.values[spec.key];
    if (value === shown(spec, settings)) continue;
    patch[spec.key] = value === '' ? null : value;
  }
  return patch;
}

/** The status shown after a plugin's name; a problem outranks an available update. */
export function statusLabel(p: PluginInfo): string | null {
  return stateLabel(p.state) ?? (p.update_available ? 'update available' : null);
}

export function originLabel(p: PluginInfo): string {
  switch (p.origin) {
    case 'builtin':
      return 'built-in';
    case 'linked':
      return 'linked (dev)';
    case 'installed':
      return p.store ?? 'installed';
  }
}

export function updateCount(plugins: PluginInfo[]): number {
  return plugins.filter((p) => p.update_available).length;
}

export function permissionText(permission: string): string {
  if (permission === 'network') return 'Network access';
  if (permission === 'ui:sessions') return "Panels see this task's sessions and subagents";
  if (permission.startsWith('exec:')) return `Runs ${permission.slice('exec:'.length)}`;
  return permission;
}

export function newPermissions(current: string[], offered: string[]): string[] {
  return offered.filter((p) => !current.includes(p));
}

export const CAPABILITY_FILTERS: { value: CapabilityKind | null; label: string }[] = [
  { value: null, label: 'All' },
  { value: 'forge', label: 'Forges' },
  { value: 'agent', label: 'Agents' },
  { value: 'task_source', label: 'Task sources' },
];

export function hitAction(hit: SearchHit): 'install' | 'update' | 'installed' {
  if (!hit.installed_version) return 'install';
  return hit.update_available ? 'update' : 'installed';
}

export interface PluginTab {
  name: string;
  title: string;
}

/** Usable plugins with own settings or agents get a settings tab, titled after their single agent if they have one. */
export function pluginTabs(
  plugins: PluginInfo[],
  withSettings: string[],
  agents: AgentInfo[],
): PluginTab[] {
  return plugins
    .filter((p) => p.state.state !== 'disabled' && p.state.state !== 'broken')
    .filter((p) => withSettings.includes(p.name) || agents.some((a) => a.plugin === p.name))
    .map((p) => {
      const own = agents.filter((a) => a.plugin === p.name);
      const title =
        own.length === 1 ? own[0].display_name : p.name.charAt(0).toUpperCase() + p.name.slice(1);
      return { name: p.name, title };
    });
}
