import { describe, expect, it } from 'vitest';
import { capabilityChips, draftFrom, settingsPatch, stateDetail, stateLabel } from './pluginsView';
import type { PluginInfo, PluginSettings } from './types';

const settings: PluginSettings = {
  schema: [
    { key: 'token', title: 'Token', type: 'secret', required: true, description: null, default: null },
    { key: 'region', title: 'Region', type: 'enum', required: false, description: null, default: 'eu', options: ['eu', 'us'] },
    { key: 'verbose', title: 'Verbose', type: 'bool', required: false, description: null, default: null },
    { key: 'limit', title: 'Limit', type: 'number', required: false, description: null, default: null },
  ],
  values: { region: 'eu' },
  secrets_set: ['token'],
};

describe('plugin view helpers', () => {
  it('labels only meaningful states', () => {
    expect(stateLabel({ state: 'ok' })).toBeNull();
    expect(stateLabel({ state: 'needs_setup', missing: ['Token'] })).toBe('needs setup');
    expect(stateDetail({ state: 'needs_setup', missing: ['Token', 'Team'] })).toBe('Missing: Token, Team');
    expect(stateDetail({ state: 'broken', reason: 'bad manifest' })).toBe('bad manifest');
  });

  it('names capabilities', () => {
    const plugin = { capabilities: [{ kind: 'forge', id: 'github', description: '' }, { kind: 'task_source', id: 'issues', description: '' }] } as PluginInfo;
    expect(capabilityChips(plugin)).toEqual(['Forge: github', 'Task source: issues']);
  });

  it('sends only changed values, new secrets and cleared secrets', () => {
    const draft = draftFrom(settings);
    expect(settingsPatch(settings, draft)).toEqual({});
    draft.values.region = 'us';
    draft.values.limit = 5;
    draft.secrets.token = 'new';
    expect(settingsPatch(settings, draft)).toEqual({ region: 'us', limit: 5, token: 'new' });
    const cleared = draftFrom(settings);
    cleared.cleared.push('token');
    cleared.values.region = '';
    expect(settingsPatch(settings, cleared)).toEqual({ token: null, region: null });
  });
});
