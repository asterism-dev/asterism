import type { CapabilityKind, PluginInfo, PluginSettings, PluginState, SettingSpec, SettingValue } from './types';

export function stateLabel(state: PluginState): string | null {
  switch (state.state) {
    case 'ok':
      return null;
    case 'needs_setup':
      return 'needs setup';
    case 'broken':
    case 'failing':
      return state.state;
  }
}

export function stateDetail(state: PluginState): string | null {
  switch (state.state) {
    case 'ok':
      return null;
    case 'needs_setup':
      return `Missing: ${state.missing.join(', ')}`;
    case 'broken':
    case 'failing':
      return state.reason;
  }
}

const KIND_LABELS: Record<CapabilityKind, string> = { forge: 'Forge', agent: 'Agent', command: 'Command', task_source: 'Task source' };

export function capabilityChips(plugin: PluginInfo): string[] {
  return plugin.capabilities.map((c) => `${KIND_LABELS[c.kind]}: ${c.id}`);
}

export interface SettingsDraft { values: Record<string, SettingValue>; secrets: Record<string, string>; cleared: string[] }

function shown(spec: SettingSpec, settings: PluginSettings): SettingValue {
  return settings.values[spec.key] ?? (spec.type === 'bool' ? false : '');
}

export function draftFrom(settings: PluginSettings): SettingsDraft {
  const values: Record<string, SettingValue> = {};
  for (const spec of settings.schema) if (spec.type !== 'secret') values[spec.key] = shown(spec, settings);
  return { values, secrets: {}, cleared: [] };
}

/** Only changes: empty text restores the default; secrets are sent only when typed or cleared. */
export function settingsPatch(settings: PluginSettings, draft: SettingsDraft): Record<string, SettingValue | null> {
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
