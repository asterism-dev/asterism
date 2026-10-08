import { describe, expect, it } from 'vitest';
import { agentSections, commandPreview, emptyForm, fromForm, toForm } from './settingsForm';
import type { AgentConfig } from './types';

const ALL = { args: true, mcp: true, hooks: true, hibernate: true };
const NONE = { args: false, mcp: false, hooks: false, hibernate: false };

const config: AgentConfig = {
  args: ['--model', 'opus'],
  env: { remove: ['AWS_*'], set: { FOO: 'bar' } },
  mcp: { mcpServers: { fs: { command: 'npx' } } },
  hooks: null,
  hibernate_after_min: null,
};

describe('settings form', () => {
  it('roundtrips a config through the form', () => {
    const form = toForm(config);
    expect(form.set).toEqual([{ key: 'FOO', value: 'bar' }]);
    expect(form.hooksText).toBe('');
    expect(fromForm(form, ALL)).toEqual({ config });
  });

  it('drops empty rows and rejects duplicate keys', () => {
    const form = {
      ...emptyForm(),
      args: ['', '  ', '--verbose'],
      remove: [' ', 'X_*'],
      set: [{ key: '', value: 'x' }],
    };
    expect(fromForm(form, ALL)).toEqual({
      config: {
        args: ['--verbose'],
        env: { remove: ['X_*'], set: {} },
        mcp: null,
        hooks: null,
        hibernate_after_min: null,
      },
    });
    const dup = {
      ...emptyForm(),
      set: [
        { key: 'A', value: '1' },
        { key: 'A', value: '2' },
      ],
    };
    expect(fromForm(dup, ALL)).toEqual({ errors: { env: 'A is set twice' } });
  });

  it('accepts variable names that clash with Object.prototype', () => {
    const form = {
      ...emptyForm(),
      set: [
        { key: 'constructor', value: '1' },
        { key: '__proto__', value: '2' },
        { key: 'toString', value: '3' },
      ],
    };
    const result = fromForm(form, ALL);
    if (!('config' in result)) throw new Error(JSON.stringify(result));
    expect(Object.entries(result.config.env.set)).toEqual([
      ['constructor', '1'],
      ['__proto__', '2'],
      ['toString', '3'],
    ]);
    expect(JSON.parse(JSON.stringify(result.config.env.set))).toEqual(
      JSON.parse('{"constructor":"1","__proto__":"2","toString":"3"}'),
    );
  });

  it('reports invalid JSON per section', () => {
    const result = fromForm({ ...emptyForm(), mcpText: '{ nope', hooksText: '[1]' }, ALL);
    expect('errors' in result && result.errors.mcp).toMatch(/^MCP servers/);
    expect('errors' in result && result.errors.hooks).toBe('Hooks must be a JSON object');
  });

  it('shows only the sections an agent supports', () => {
    const claude = {
      name: 'claude',
      available: true,
      display_name: 'Claude Code',
      settings: ['args', 'mcp', 'hooks', 'hibernate'],
      plugin: 'claude',
    } as const;
    expect(agentSections({ ...claude, settings: [...claude.settings] })).toEqual({
      args: true,
      mcp: true,
      hooks: true,
      hibernate: true,
    });
    expect(agentSections(undefined)).toEqual(NONE);
  });

  it('ignores options an agent does not support', () => {
    const form = { ...toForm(config), hooksText: '{ broken' };
    expect(fromForm(form, NONE)).toEqual({
      config: { ...config, args: [], mcp: null, hooks: null },
    });
  });

  it('previews the command line with quoting', () => {
    const form = { ...emptyForm(), args: ['--model', 'opus 4'], mcpText: '{"mcpServers":{}}' };
    expect(commandPreview('claude', form)).toBe("claude … --model 'opus 4'");
  });
});

describe('hibernate setting', () => {
  const sections = { ...NONE, hibernate: true };
  const base: AgentConfig = { args: [], env: { remove: [], set: {} }, mcp: null, hooks: null };

  it('round-trips minutes and keeps empty as default', () => {
    const form = toForm({ ...base, hibernate_after_min: 5 });
    expect(form.hibernateAfter).toBe('5');
    expect(fromForm({ ...form, hibernateAfter: '' }, sections)).toMatchObject({
      config: { hibernate_after_min: null },
    });
    expect(fromForm({ ...form, hibernateAfter: '0' }, sections)).toMatchObject({
      config: { hibernate_after_min: 0 },
    });
  });

  it('rejects non-integers', () => {
    const form = { ...emptyForm(), hibernateAfter: '1.5' };
    expect(fromForm(form, sections)).toMatchObject({ errors: { hibernate: expect.any(String) } });
  });

  it('drops the value for agents without the setting', () => {
    const form = { ...emptyForm(), hibernateAfter: '5' };
    expect(fromForm(form, NONE)).toMatchObject({ config: { hibernate_after_min: null } });
  });
});
