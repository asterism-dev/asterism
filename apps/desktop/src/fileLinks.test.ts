import { describe, expect, it } from 'vitest';
import { findFileLinks } from './fileLinks';

describe('findFileLinks', () => {
  it('finds relative paths with an extension and their line', () => {
    expect(findFileLinks('edit src/api.ts:42 now')).toEqual([{ start: 5, end: 18, path: 'src/api.ts', line: 42 }]);
    expect(findFileLinks('a.ts:42:7')).toEqual([{ start: 0, end: 9, path: 'a.ts', line: 42 }]);
    expect(findFileLinks('Cargo.toml')).toEqual([{ start: 0, end: 10, path: 'Cargo.toml' }]);
  });

  it('finds prefixed paths without an extension', () => {
    expect(findFileLinks('cat /etc/hosts')).toEqual([{ start: 4, end: 14, path: '/etc/hosts' }]);
    expect(findFileLinks('~/.zshrc')).toEqual([{ start: 0, end: 8, path: '~/.zshrc' }]);
    expect(findFileLinks('run ./build')).toEqual([{ start: 4, end: 11, path: './build' }]);
    expect(findFileLinks('../x/y')).toEqual([{ start: 0, end: 6, path: '../x/y' }]);
  });

  it('ignores prose, directories, versions and decimals', () => {
    expect(findFileLinks('and/or src/components')).toEqual([]);
    expect(findFileLinks('version v0.3.0 took 1.5s')).toEqual([]);
    expect(findFileLinks('done.')).toEqual([]);
  });

  it('leaves URLs to the web-links addon', () => {
    expect(findFileLinks('see https://x.com/a.js')).toEqual([]);
  });

  it('excludes surrounding punctuation', () => {
    expect(findFileLinks('see src/a.ts.')).toEqual([{ start: 4, end: 12, path: 'src/a.ts' }]);
    expect(findFileLinks('(src/a.ts)')).toEqual([{ start: 1, end: 9, path: 'src/a.ts' }]);
    expect(findFileLinks('`b.rs:3`')).toEqual([{ start: 1, end: 7, path: 'b.rs', line: 3 }]);
  });

  it('treats line 0 as no line', () => {
    expect(findFileLinks('a.ts:0')).toEqual([{ start: 0, end: 6, path: 'a.ts' }]);
  });
});
