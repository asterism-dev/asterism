import { describe, expect, it } from 'vitest';
import { capabilityChips, draftFrom, hitAction, newPermissions, originLabel, permissionText, pluginTabs, settingsPatch, stateDetail, stateLabel, statusLabel, updateCount } from './pluginsView';
import type { AgentInfo, PluginInfo, PluginSettings, SearchHit } from './types';

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
  it('gives usable plugins with settings or agents a tab', () => {
    const plugin = (name: string, state: PluginInfo['state'] = { state: 'ok' }) => ({ name, state }) as PluginInfo;
    const agent = (name: string, display_name: string, owner: string) => ({ name, display_name, plugin: owner }) as AgentInfo;
    const plugins = [plugin('claude'), plugin('linear', { state: 'needs_setup', missing: ['API key'] }), plugin('github'), plugin('multi'), plugin('off', { state: 'disabled' })];
    const agents = [agent('claude', 'Claude Code', 'claude'), agent('a', 'A', 'multi'), agent('b', 'B', 'multi'), agent('x', 'X', 'off')];
    expect(pluginTabs(plugins, ['linear', 'off'], agents)).toEqual([
      { name: 'claude', title: 'Claude Code' },
      { name: 'linear', title: 'Linear' },
      { name: 'multi', title: 'Multi' },
    ]);
  });

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

function plugin(over: Partial<PluginInfo>): PluginInfo {
  return {
    name: 'one', version: '1.0.0', description: '', origin: 'installed', path: '/p', capabilities: [], permissions: [],
    state: { state: 'ok' }, backend: null, store: 'acme', update_available: false, previous_version: null, ...over,
  };
}

describe('store helpers', () => {
  it('shows status only when meaningful', () => {
    expect(statusLabel(plugin({}))).toBeNull();
    expect(statusLabel(plugin({ update_available: true }))).toBe('update available');
    expect(statusLabel(plugin({ state: { state: 'disabled' }, update_available: true }))).toBe('disabled');
    expect(stateDetail({ state: 'disabled' })).toBeNull();
  });

  it('names origins and counts updates', () => {
    expect(originLabel(plugin({}))).toBe('acme');
    expect(originLabel(plugin({ origin: 'linked' }))).toBe('linked (dev)');
    expect(originLabel(plugin({ origin: 'builtin', store: null }))).toBe('built-in');
    expect(updateCount([plugin({ update_available: true }), plugin({}), plugin({ update_available: true, state: { state: 'disabled' } })])).toBe(2);
  });

  it('explains permissions and finds new ones', () => {
    expect(permissionText('exec:glab')).toBe('Runs glab');
    expect(permissionText('network')).toBe('Network access');
    expect(permissionText('fs:read')).toBe('fs:read');
    expect(newPermissions(['network'], ['network', 'exec:gh'])).toEqual(['exec:gh']);
  });

  it('picks the action for a search hit', () => {
    const hit: SearchHit = { store: 'acme', name: 'one', description: '', tags: [], installed_version: null, update_available: false, linked: false };
    expect(hitAction(hit)).toBe('install');
    expect(hitAction({ ...hit, installed_version: '1.0.0' })).toBe('installed');
    expect(hitAction({ ...hit, installed_version: '1.0.0', update_available: true })).toBe('update');
  });
});
