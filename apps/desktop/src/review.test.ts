import { describe, expect, it } from 'vitest';
import {
  defaultTarget,
  editorButtons,
  extendDataFor,
  formatTime,
  isLargeDiff,
  splitPatch,
  startsCollapsed,
  buildTree,
  checkPrompt,
  codeExcerpt,
  diffBar,
  fileStatus,
  filterFiles,
  flattenTree,
  groupByDay,
  logExcerpt,
  logSections,
  pollInterval,
  stripAnsi,
  timeline,
  timeValue,
  type FileDiff,
  type TreeNode,
} from './review';
import type { CheckRun, ReviewResult, ReviewThread, Session } from './types';

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
    status: 'modified' as const,
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

const mk = (path: string, patch = ''): FileDiff => ({
  path,
  patch,
  additions: 0,
  deletions: 0,
  status: 'modified',
});

describe('fileStatus', () => {
  it('reads the header', () => {
    expect(
      fileStatus(
        'diff --git a/x b/x\nnew file mode 100644\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+a\n',
      ),
    ).toBe('added');
    expect(fileStatus('diff --git a/x b/x\ndeleted file mode 100644\n')).toBe('deleted');
    expect(
      fileStatus('diff --git a/x b/y\nsimilarity index 100%\nrename from x\nrename to y\n'),
    ).toBe('renamed');
    expect(
      fileStatus('diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-new file mode\n+a\n'),
    ).toBe('modified');
  });
  it('is set by splitPatch', () => {
    expect(
      splitPatch(
        'diff --git a/x b/x\nnew file mode 100644\n--- /dev/null\n+++ b/x\n@@ -0,0 +1 @@\n+a\n',
      )[0].status,
    ).toBe('added');
  });
});

describe('buildTree / flattenTree / filterFiles', () => {
  const files = ['a/b/c/x.ts', 'a/b/c/y.ts', 'a/d.ts', 'z.md'].map((p) => mk(p));
  it('collapses single-child folder chains and puts folders first', () => {
    const tree = buildTree(files);
    expect(tree.map((n) => n.name)).toEqual(['a', 'z.md']);
    const a = tree[0] as Extract<TreeNode, { kind: 'dir' }>;
    expect(a.children.map((n) => n.name)).toEqual(['b/c', 'd.ts']);
    expect((a.children[0] as Extract<TreeNode, { kind: 'dir' }>).path).toBe('a/b/c');
  });
  it('flattens with depth and hides collapsed folders', () => {
    const rows = flattenTree(buildTree(files), new Set(['a/b/c']));
    expect(rows.map((r) => [r.node.name, r.depth])).toEqual([
      ['a', 0],
      ['b/c', 1],
      ['d.ts', 1],
      ['z.md', 0],
    ]);
  });
  it('filters case-insensitively on the path', () => {
    expect(filterFiles(files, 'B/C').map((f) => f.path)).toEqual(['a/b/c/x.ts', 'a/b/c/y.ts']);
    expect(filterFiles(files, '  ')).toHaveLength(4);
  });
});

describe('groupByDay', () => {
  it('groups by the author date and keeps order', () => {
    const c = (sha: string, date: string) => ({ sha, short: sha, subject: sha, author: 'a', date });
    const groups = groupByDay([
      c('3', '2026-10-09T10:00:00+02:00'),
      c('2', '2026-10-09T08:00:00+02:00'),
      c('1', '2026-10-08T20:00:00+02:00'),
    ]);
    expect(groups.map((g) => [g.day, g.commits.map((x) => x.sha)])).toEqual([
      ['2026-10-09', ['3', '2']],
      ['2026-10-08', ['1']],
    ]);
  });
});

describe('logs', () => {
  const ESC = String.fromCharCode(27);
  const log = [
    '2026-10-09T10:00:00.1234567Z ##[group]Run cargo build',
    '2026-10-09T10:00:01.0000000Z cargo build',
    '2026-10-09T10:00:02.0000000Z ##[endgroup]',
    `2026-10-09T10:00:03.0000000Z ${ESC}[32mCompiling${ESC}[0m x`,
    '2026-10-09T10:00:04.0000000Z ##[group]Run cargo test',
    '2026-10-09T10:00:05.0000000Z test a ... FAILED',
    '2026-10-09T10:00:06.0000000Z ##[error]Process completed with exit code 101.',
    '',
  ].join('\n');
  it('ignores a BOM and CRLF line endings', () => {
    const s = logSections('\uFEFF##[group]Run a\r\nhello\r\n##[endgroup]\r\n');
    expect(s.map((x) => [x.title, x.lines])).toEqual([['Run a', ['hello']]]);
  });
  it('strips ANSI and timestamps and splits on groups', () => {
    const s = logSections(log);
    expect(s.map((x) => [x.title, x.failed])).toEqual([
      ['Run cargo build', false],
      ['Run cargo test', true],
    ]);
    expect(s[0].lines).toEqual(['cargo build', 'Compiling x']);
    expect(stripAnsi(`${ESC}[1;31mred${ESC}[0m`)).toBe('red');
  });
  it('excerpts the failed sections, else the tail', () => {
    expect(logExcerpt(logSections(log))).toBe(
      'Run cargo test\ntest a ... FAILED\n##[error]Process completed with exit code 101.',
    );
    const ok = logSections('a\nb\nc');
    expect(logExcerpt(ok, 2)).toBe('b\nc');
  });
  it('builds an agent prompt', () => {
    expect(checkPrompt('test', 'boom')).toBe(
      'The CI check "test" failed. Log excerpt:\n\n```\nboom\n```\n\nFind and fix the cause.',
    );
  });
});

describe('diffBar / pollInterval / timeValue', () => {
  it('splits five squares', () => {
    expect(diffBar(0, 0)).toEqual(['none', 'none', 'none', 'none', 'none']);
    expect(diffBar(10, 10)).toEqual(['add', 'add', 'add', 'del', 'del']);
    expect(diffBar(4110, 187)).toEqual(['add', 'add', 'add', 'add', 'add']);
    expect(diffBar(1, 99)).toEqual(['del', 'del', 'del', 'del', 'del']);
  });
  it('polls fast while checks run', () => {
    const r = (status: CheckRun['status']) => ({ checks: [{ status }] }) as unknown as ReviewResult;
    expect(pollInterval(r('running'))).toBe(15_000);
    expect(pollInterval(r('queued'))).toBe(15_000);
    expect(pollInterval(r('done'))).toBe(60_000);
    expect(pollInterval(null)).toBe(60_000);
  });
  it('reads epoch seconds and ISO strings', () => {
    expect(timeValue('1700000000')).toBe(1_700_000_000_000);
    expect(timeValue('2026-10-09T00:00:00Z')).toBe(Date.parse('2026-10-09T00:00:00Z'));
    expect(timeValue('')).toBe(0);
  });
});

describe('codeExcerpt / timeline', () => {
  const patch =
    'diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,4 +1,4 @@\n l1\n l2\n-old\n+new\n l4\n';
  it('returns the lines up to the target', () => {
    expect(codeExcerpt(patch, 'new', 3)).toEqual([
      { tag: ' ', text: 'l1' },
      { tag: ' ', text: 'l2' },
      { tag: '-', text: 'old' },
      { tag: '+', text: 'new' },
    ]);
    expect(codeExcerpt(patch, 'old', 3, 1)).toEqual([
      { tag: ' ', text: 'l2' },
      { tag: '-', text: 'old' },
    ]);
    expect(codeExcerpt(patch, 'new', 99)).toEqual([]);
  });
  it('merges conversation items and threads by time', () => {
    const review = {
      conversation: [
        {
          id: 'c',
          kind: 'comment',
          author: 'a',
          body: 'x',
          created_at: '2026-10-09T10:00:00Z',
          state: null,
        },
      ],
      threads: [
        {
          id: 't',
          path: 'a.rs',
          line: 1,
          side: 'new',
          outdated: false,
          resolved: false,
          local: true,
          pending: false,
          comments: [{ id: '1', author: 'local', body: 'y', created_at: '1700000000' }],
        },
      ],
    } as unknown as ReviewResult;
    expect(timeline(review).map((e) => (e.kind === 'item' ? e.item.id : e.thread.id))).toEqual([
      't',
      'c',
    ]);
  });
});
