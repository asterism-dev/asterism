import { describe, expect, it } from 'vitest';
import {
  defaultTarget,
  editorButtons,
  extendDataFor,
  formatTime,
  isLargeDiff,
  splitPatch,
  startsCollapsed,
} from './review';
import type { ReviewThread, Session } from './types';

const PATCH = `diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-old
+new
 same
diff --git a/b.rs b/b.rs
new file mode 100644
--- /dev/null
+++ b/b.rs
@@ -0,0 +1 @@
+added
`;

const GIT = `diff --git "a/caf\\303\\251.rs" "b/caf\\303\\251.rs"
index 587be6b..975fbec 100644
--- "a/caf\\303\\251.rs"
+++ "b/caf\\303\\251.rs"
@@ -1 +1 @@
-x
+y
diff --git a/my file.rs b/my file.rs
index b3addbe..7e43fc4 100644
--- a/my file.rs\t
+++ b/my file.rs\t
@@ -1,2 +1,2 @@
 one
-last
\\ No newline at end of file
+last2
\\ No newline at end of file
diff --git a/old.rs b/new.rs
similarity index 100%
rename from old.rs
rename to new.rs
diff --git a/u.rs b/u.rs
index 66b4ba3..f971f2b 100644
--- a/u.rs
+++ b/u.rs
@@ -1 +1,2 @@
 é line
+z
`;

const thread = (id: string, line: number, side: 'old' | 'new', resolved = false): ReviewThread => ({
  id,
  path: 'a.rs',
  line,
  side,
  outdated: false,
  resolved,
  local: false,
  pending: false,
  comments: [],
});

describe('splitPatch', () => {
  it('splits per file with counts', () => {
    const files = splitPatch(PATCH);
    expect(files.map((f) => [f.path, f.additions, f.deletions])).toEqual([
      ['a.rs', 1, 1],
      ['b.rs', 1, 0],
    ]);
    expect(files[1].patch.startsWith('diff --git a/b.rs')).toBe(true);
  });
  it('returns nothing for an empty patch', () => expect(splitPatch('')).toEqual([]));
  it('matches the daemon paths: quoted non-ASCII, trailing tab, pure rename', () => {
    expect(splitPatch(GIT).map((f) => f.path)).toEqual(['café.rs', 'my file.rs', 'new.rs', 'u.rs']);
  });
  it('does not count a "-- x" removed line or "\\ No newline" as header', () => {
    const [f] = splitPatch('diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- x\n+++ y\n');
    expect([f.additions, f.deletions]).toEqual([1, 1]);
  });
});

describe('extendDataFor', () => {
  it('groups threads by side and line, skipping outdated ones', () => {
    const outdated = { ...thread('x', 1, 'new'), outdated: true };
    const data = extendDataFor([
      thread('a', 1, 'new'),
      thread('b', 1, 'new'),
      thread('c', 2, 'old'),
      outdated,
    ]);
    expect(data.newFile['1'].data.map((t) => t.id)).toEqual(['a', 'b']);
    expect(data.oldFile['2'].data.map((t) => t.id)).toEqual(['c']);
  });
});

describe('editorButtons', () => {
  it('offers only a local comment without forge reviews', () => {
    expect(editorButtons('local', true, false).map((b) => b.label)).toEqual(['Comment']);
    expect(editorButtons('pr', false, false).map((b) => b.label)).toEqual(['Comment']);
  });
  it('offers local, single and review on a pull request', () => {
    expect(editorButtons('pr', true, false).map((b) => [b.label, b.target])).toEqual([
      ['Local comment', 'local'],
      ['Comment', 'single'],
      ['Start a review', 'review'],
    ]);
    expect(editorButtons('pr', true, true).pop()?.label).toBe('Add review comment');
  });
});

describe('defaultTarget', () => {
  const s = (
    id: number,
    status: Session['status'],
    kind: Session['kind'] = { type: 'agent', name: 'claude' },
  ): Session => ({ id, task_id: 1, kind, status });
  it('prefers the newest idle agent session of the task', () => {
    expect(
      defaultTarget(
        [s(1, 'idle'), s(2, 'working'), s(3, 'idle'), s(4, 'idle', { type: 'shell' })],
        1,
      ),
    ).toBe(3);
  });
  it('falls back to any live agent session, then to a new one', () => {
    expect(defaultTarget([s(2, 'working')], 1)).toBe(2);
    expect(defaultTarget([s(5, 'exited')], 1)).toBeNull();
  });
});

describe('startsCollapsed', () => {
  const file = (additions: number, deletions: number) => ({
    path: 'a',
    patch: '',
    additions,
    deletions,
  });
  it('collapses large diffs and files past the first 30', () => {
    expect(startsCollapsed(file(600, 400), 0)).toBe(false);
    expect(startsCollapsed(file(600, 401), 0)).toBe(true);
    expect(isLargeDiff(file(600, 401))).toBe(true);
    expect(startsCollapsed(file(1, 1), 29)).toBe(false);
    expect(startsCollapsed(file(1, 1), 30)).toBe(true);
    expect(isLargeDiff(file(1, 1))).toBe(false);
  });
});

describe('formatTime', () => {
  it('formats forge ISO strings and local epoch seconds alike', () => {
    expect(formatTime('1700000000')).toBe(formatTime('2023-11-14T22:13:20Z'));
    expect(formatTime('1700000000')).not.toBe('1700000000');
  });
  it('leaves unparseable values alone', () => expect(formatTime('')).toBe(''));
});
