import { describe, expect, it } from 'vitest';
import { escapeHtml, languageFor, renderCode } from './fileView';

describe('fileView', () => {
  it('picks the language from the extension or the file name', () => {
    expect(languageFor('src/api.ts')).toBe('ts');
    expect(languageFor('crates/x/Cargo.toml')).toBe('toml');
    expect(languageFor('Makefile')).toBe('makefile');
    expect(languageFor('~/.zshrc')).toBeNull();
    expect(languageFor('notes.unknownext')).toBeNull();
  });

  it('escapes html', () => {
    expect(escapeHtml('<a href="x">&</a>')).toBe('&lt;a href="x"&gt;&amp;&lt;/a&gt;');
  });

  it('highlights known languages and escapes everything else', () => {
    expect(renderCode('a.ts', 'const x = 1;')).toContain('hljs-keyword');
    expect(renderCode('a.unknownext', '<script>')).toBe('&lt;script&gt;');
  });

  it('skips highlighting for large content', () => {
    expect(renderCode('a.ts', 'const x = 1;\n'.repeat(20_000))).not.toContain('hljs-');
  });
});
