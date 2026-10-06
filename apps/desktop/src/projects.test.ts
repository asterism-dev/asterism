import { describe, expect, it } from 'vitest';
import { relativeTime, repoNameError, sortProjects, sortTasks, sourceOwnerRepo, targetPath, visibilityChoices } from './projects';
import type { GithubStatus, Project, Task } from './types';

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

const proj = (id: number, name: string, created_at: number): Project => ({ id, name, path: `/p/${id}`, created_at, default_base: null });
const tsk = (id: number, project_id: number, title: string, created_at: number, last_activity_at: number): Task => ({
  id, project_id, title, slug: `${id}`, branch: `b${id}`, base_branch: 'main', worktree_path: `/wt/${id}`, prompt: null,
  archived: false, created_at, last_activity_at,
});

describe('sorting', () => {
  const tasks = [tsk(1, 1, 'beta', 100, 500), tsk(2, 1, 'Alpha', 200, 300), tsk(3, 2, 'task 10', 300, 400), tsk(4, 2, 'task 9', 300, 100)];
  const projects = [proj(1, 'zeta', 10), proj(2, 'Api', 20), proj(3, 'empty', 30)];
  const ids = (items: { id: number }[]) => items.map((i) => i.id);

  it('sorts tasks by name, activity or newest first', () => {
    expect(ids(sortTasks(tasks, 'alphabetical'))).toEqual([2, 1, 4, 3]);
    expect(ids(sortTasks(tasks, 'activity'))).toEqual([1, 3, 2, 4]);
    expect(ids(sortTasks(tasks, 'added'))).toEqual([4, 3, 2, 1]);
  });

  it('sorts projects by their most active task, falling back to creation', () => {
    expect(ids(sortProjects(projects, tasks, 'alphabetical'))).toEqual([2, 3, 1]);
    expect(ids(sortProjects(projects, tasks, 'activity'))).toEqual([1, 2, 3]);
    expect(ids(sortProjects(projects, tasks, 'added'))).toEqual([3, 2, 1]);
  });
});

describe('relativeTime', () => {
  it('uses the largest whole unit', () => {
    expect(relativeTime(1000, 1030)).toBe('now');
    expect(relativeTime(1000, 1000 + 5 * 60)).toBe('5m');
    expect(relativeTime(1000, 1000 + 2 * 3600 + 59)).toBe('2h');
    expect(relativeTime(1000, 1000 + 3 * 86400)).toBe('3d');
    expect(relativeTime(1000, 1000 + 15 * 86400)).toBe('2w');
    expect(relativeTime(1000, 1000 + 400 * 86400)).toBe('1y');
    expect(relativeTime(2000, 1000)).toBe('now');
  });
});
