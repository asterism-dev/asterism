import { describe, expect, it } from 'vitest';
import { agentSections, commandPreview, emptyForm, fromForm, toForm } from './settingsForm';
import type { AgentConfig } from './types';

const ALL = { args: true, mcp: true, hooks: true };
const NONE = { args: false, mcp: false, hooks: false };

const config: AgentConfig = {
  args: ['--model', 'opus'],
  env: { remove: ['AWS_*'], set: { FOO: 'bar' } },
  mcp: { mcpServers: { fs: { command: 'npx' } } },
  hooks: null,
};

describe('settings form', () => {
  it('roundtrips a config through the form', () => {
    const form = toForm(config);
    expect(form.set).toEqual([{ key: 'FOO', value: 'bar' }]);
    expect(form.hooksText).toBe('');
    expect(fromForm(form, ALL)).toEqual({ config });
  });

  it('drops empty rows and rejects duplicate keys', () => {
    const form = { ...emptyForm(), args: ['', '  ', '--verbose'], remove: [' ', 'X_*'], set: [{ key: '', value: 'x' }] };
    expect(fromForm(form, ALL)).toEqual({
      config: { args: ['--verbose'], env: { remove: ['X_*'], set: {} }, mcp: null, hooks: null },
    });
    const dup = { ...emptyForm(), set: [{ key: 'A', value: '1' }, { key: 'A', value: '2' }] };
    expect(fromForm(dup, ALL)).toEqual({ errors: { env: 'A is set twice' } });
  });

  it('accepts variable names that clash with Object.prototype', () => {
    const form = { ...emptyForm(), set: [{ key: 'constructor', value: '1' }, { key: '__proto__', value: '2' }, { key: 'toString', value: '3' }] };
    const result = fromForm(form, ALL);
    if (!('config' in result)) throw new Error(JSON.stringify(result));
    expect(Object.entries(result.config.env.set)).toEqual([['constructor', '1'], ['__proto__', '2'], ['toString', '3']]);
    expect(JSON.parse(JSON.stringify(result.config.env.set))).toEqual(JSON.parse('{"constructor":"1","__proto__":"2","toString":"3"}'));
  });

  it('reports invalid JSON per section', () => {
    const result = fromForm({ ...emptyForm(), mcpText: '{ nope', hooksText: '[1]' }, ALL);
    expect('errors' in result && result.errors.mcp).toMatch(/^MCP servers/);
    expect('errors' in result && result.errors.hooks).toBe('Hooks must be a JSON object');
  });

  it('shows only the sections an agent supports', () => {
    const claude = { name: 'claude', available: true, display_name: 'Claude Code', settings: ['args', 'mcp', 'hooks'], plugin: 'claude' } as const;
    expect(agentSections({ ...claude, settings: [...claude.settings] })).toEqual({ args: true, mcp: true, hooks: true });
    expect(agentSections(undefined)).toEqual({ args: false, mcp: false, hooks: false });
  });

  it('ignores options an agent does not support', () => {
    const form = { ...toForm(config), hooksText: '{ broken' };
    expect(fromForm(form, NONE)).toEqual({ config: { ...config, args: [], mcp: null, hooks: null } });
  });

  it('previews the command line with quoting', () => {
    const form = { ...emptyForm(), args: ['--model', 'opus 4'], mcpText: '{"mcpServers":{}}' };
    expect(commandPreview('claude', form)).toBe("claude … --model 'opus 4'");
  });
});
