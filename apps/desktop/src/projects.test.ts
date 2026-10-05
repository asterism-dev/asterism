import { describe, expect, it } from 'vitest';
import { repoNameError, sourceOwnerRepo, targetPath, visibilityChoices } from './projects';
import type { GithubStatus } from './types';

const status: GithubStatus = { available: true, logged_in: true, login: 'me', orgs: ['acme'], error: null };

describe('project helpers', () => {
  it('validates repository names like the daemon', () => {
    expect(repoNameError('ok.name_1-x')).toBeNull();
    for (const bad of ['', '.', '..', '-x', 'a/b', 'a b', 'ä']) expect(repoNameError(bad)).not.toBeNull();
  });

  it('rejects sources with invalid characters early', () => {
    expect(sourceOwnerRepo('--upload-pack=evil:a/b')).toBeNull();
    expect(sourceOwnerRepo('-x:a/b')).toBeNull();
    expect(sourceOwnerRepo('git@host:a/b c')).toBeNull();
    expect(sourceOwnerRepo('https://github.com/acme/api/tr ee/main')).toBeNull();
  });

  it('reads owner and repo from shorthand and URLs', () => {
    expect(sourceOwnerRepo('acme/api')).toEqual({ owner: 'acme', repo: 'api' });
    expect(sourceOwnerRepo('git@github.com:acme/api.git')).toEqual({ owner: 'acme', repo: 'api' });
    expect(sourceOwnerRepo('https://gitlab.com/group/sub/tool/')).toEqual({ owner: 'sub', repo: 'tool' });
    expect(sourceOwnerRepo('acme')).toBeNull();
    expect(sourceOwnerRepo('acme/..')).toBeNull();
  });

  it('takes first two segments for GitHub URLs', () => {
    expect(sourceOwnerRepo('https://github.com/acme/api/tree/main')).toEqual({ owner: 'acme', repo: 'api' });
    expect(sourceOwnerRepo('ssh://git@github.com/acme/api.git')).toEqual({ owner: 'acme', repo: 'api' });
  });

  it('previews target paths', () => {
    expect(targetPath('/h/repos', 'acme', 'api')).toBe('/h/repos/acme/api');
    expect(targetPath('/h/repos/', 'local', 'x')).toBe('/h/repos/local/x');
  });

  it('offers internal only for organizations (case-insensitive)', () => {
    expect(visibilityChoices('me', status)).toEqual(['private', 'public']);
    expect(visibilityChoices('acme', status)).toEqual(['private', 'public', 'internal']);
    expect(visibilityChoices('Acme', status)).toEqual(['private', 'public', 'internal']);
    expect(visibilityChoices('Me', status)).toEqual(['private', 'public']);
  });

  it('handles null login in visibility choices', () => {
    const noLogin: GithubStatus = { available: true, logged_in: false, login: null, orgs: ['acme'], error: null };
    expect(visibilityChoices('acme', noLogin)).toEqual(['private', 'public', 'internal']);
    expect(visibilityChoices('other', noLogin)).toEqual(['private', 'public']);
  });
});
