import { describe, expect, it } from 'vitest';
import { repoNameError, sourceOwnerRepo, targetPath, visibilityChoices } from './projects';
import type { GithubStatus } from './types';

const status: GithubStatus = { available: true, logged_in: true, login: 'me', orgs: ['acme'], error: null };

describe('project helpers', () => {
  it('validates repository names like the daemon', () => {
    expect(repoNameError('ok.name_1-x')).toBeNull();
    for (const bad of ['', '.', '..', '-x', 'a/b', 'a b', 'ä']) expect(repoNameError(bad)).not.toBeNull();
  });

  it('reads owner and repo from shorthand and URLs', () => {
    expect(sourceOwnerRepo('acme/api')).toEqual({ owner: 'acme', repo: 'api' });
    expect(sourceOwnerRepo('git@github.com:acme/api.git')).toEqual({ owner: 'acme', repo: 'api' });
    expect(sourceOwnerRepo('https://gitlab.com/group/sub/tool/')).toEqual({ owner: 'sub', repo: 'tool' });
    expect(sourceOwnerRepo('acme')).toBeNull();
    expect(sourceOwnerRepo('acme/..')).toBeNull();
  });

  it('previews target paths', () => {
    expect(targetPath('/h/repos', 'acme', 'api')).toBe('/h/repos/acme/api');
    expect(targetPath('/h/repos/', 'local', 'x')).toBe('/h/repos/local/x');
  });

  it('offers internal only for organizations', () => {
    expect(visibilityChoices('me', status)).toEqual(['private', 'public']);
    expect(visibilityChoices('acme', status)).toEqual(['private', 'public', 'internal']);
  });
});
