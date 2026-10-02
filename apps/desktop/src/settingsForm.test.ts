import { describe, expect, it } from 'vitest';
import { commandPreview, emptyForm, fromForm, supportsAgentOptions, toForm } from './settingsForm';
import type { AgentConfig } from './types';

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
    expect(fromForm(form, true)).toEqual({ config });
  });

  it('drops empty rows and rejects duplicate keys', () => {
    const form = { ...emptyForm(), args: ['', '--verbose'], remove: [' ', 'X_*'], set: [{ key: '', value: 'x' }] };
    expect(fromForm(form, true)).toEqual({
      config: { args: ['--verbose'], env: { remove: ['X_*'], set: {} }, mcp: null, hooks: null },
    });
    const dup = { ...emptyForm(), set: [{ key: 'A', value: '1' }, { key: 'A', value: '2' }] };
    expect(fromForm(dup, true)).toEqual({ errors: { env: 'A is set twice' } });
  });

  it('reports invalid JSON per section', () => {
    const result = fromForm({ ...emptyForm(), mcpText: '{ nope', hooksText: '[1]' }, true);
    expect('errors' in result && result.errors.mcp).toMatch(/^MCP servers/);
    expect('errors' in result && result.errors.hooks).toBe('Hooks must be a JSON object');
  });

  it('ignores agent-only options for shells and commands', () => {
    expect(supportsAgentOptions('claude')).toBe(true);
    expect(supportsAgentOptions('shell')).toBe(false);
    const form = { ...toForm(config), hooksText: '{ broken' };
    expect(fromForm(form, false)).toEqual({ config: { ...config, args: [], mcp: null, hooks: null } });
  });

  it('previews the command line with quoting', () => {
    const form = { ...emptyForm(), args: ['--model', 'opus 4'], mcpText: '{"mcpServers":{}}' };
    expect(commandPreview('claude', form)).toBe("claude --settings … --mcp-config … --model 'opus 4'");
  });
});
