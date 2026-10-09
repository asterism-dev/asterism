import type {
  CheckRun,
  CommentTarget,
  ConversationItem,
  DiffSide,
  ReviewCommit,
  ReviewResult,
  ReviewSource,
  ReviewThread,
  Session,
} from './types';

export interface FileDiff {
  path: string;
  patch: string;
  additions: number;
  deletions: number;
  status: FileStatus;
}

// Mirrors `unquote` in crates/asterism/src/review.rs.
function unquote(raw: string): string {
  raw = raw.replace(/\t+$/, '');
  if (raw.length < 2 || !raw.startsWith('"') || !raw.endsWith('"')) return raw;
  const inner = new TextEncoder().encode(raw.slice(1, -1));
  const out: number[] = [];
  for (let i = 0; i < inner.length; i++) {
    if (inner[i] !== 0x5c) {
      out.push(inner[i]);
      continue;
    }
    const d = inner[++i];
    if (d === undefined) break;
    if (d >= 0x30 && d <= 0x37) {
      let n = d - 0x30;
      for (let k = 0; k < 2 && inner[i + 1] >= 0x30 && inner[i + 1] <= 0x37; k++)
        n = (n * 8 + inner[++i] - 0x30) & 0xff;
      out.push(n);
    } else out.push(d === 0x74 ? 0x09 : d === 0x6e ? 0x0a : d);
  }
  return new TextDecoder().decode(new Uint8Array(out));
}

const stripPrefix = (p: string) => (p.includes('/') ? p.slice(p.indexOf('/') + 1) : p);

// Mirrors `patch_path` in crates/asterism/src/review.rs: threads and viewed files are keyed by this path.
function patchPath(header: string[]): string {
  const side = (prefix: string) => {
    const line = header.find((l) => l.startsWith(prefix));
    if (line === undefined) return undefined;
    const p = unquote(line.slice(prefix.length));
    return p === '/dev/null' ? undefined : stripPrefix(p);
  };
  const renamed = header.find((l) => l.startsWith('rename to '));
  const found =
    side('+++ ') ?? side('--- ') ?? (renamed && unquote(renamed.slice('rename to '.length)));
  if (found) return found;
  const line = header[0]?.slice('diff --git '.length) ?? '';
  const at = line.endsWith('"') ? line.lastIndexOf('"b/') : line.lastIndexOf(' b/') + 1;
  return at > 0 ? stripPrefix(unquote(line.slice(at))) : '';
}

export function splitPatch(patch: string): FileDiff[] {
  return patch
    .split(/^(?=diff --git )/m)
    .filter((chunk) => chunk.startsWith('diff --git '))
    .map((chunk) => {
      const lines = chunk.split('\n');
      const hunk = lines.findIndex((l) => l.startsWith('@@'));
      const header = hunk < 0 ? lines : lines.slice(0, hunk);
      let additions = 0;
      let deletions = 0;
      if (hunk >= 0) {
        for (const l of lines.slice(hunk)) {
          if (l.startsWith('+')) additions++;
          else if (l.startsWith('-')) deletions++;
        }
      }
      return {
        path: patchPath(header),
        patch: chunk,
        additions,
        deletions,
        status: fileStatus(chunk),
      };
    });
}

export function threadsByFile(threads: ReviewThread[]): Map<string, ReviewThread[]> {
  const map = new Map<string, ReviewThread[]>();
  for (const t of threads) map.set(t.path, [...(map.get(t.path) ?? []), t]);
  return map;
}

type LineThreads = Record<string, { data: ReviewThread[] }>;

/** The `extendData` prop of `@git-diff-view/vue`: threads per side, keyed by line number. */
export function extendDataFor(threads: ReviewThread[]): {
  oldFile: LineThreads;
  newFile: LineThreads;
} {
  const out = { oldFile: {} as LineThreads, newFile: {} as LineThreads };
  for (const t of threads.filter((t) => !t.outdated)) {
    const side = t.side === 'old' ? out.oldFile : out.newFile;
    (side[String(t.line)] ??= { data: [] }).data.push(t);
  }
  return out;
}

export function editorButtons(
  source: ReviewSource,
  reviewsSupported: boolean,
  pending: boolean,
): { label: string; target: CommentTarget }[] {
  if (source === 'local' || !reviewsSupported) return [{ label: 'Comment', target: 'local' }];
  return [
    { label: 'Local comment', target: 'local' },
    { label: 'Comment', target: 'single' },
    { label: pending ? 'Add review comment' : 'Start a review', target: 'review' },
  ];
}

export function agentSessions(sessions: Session[], taskId: number): Session[] {
  return sessions.filter(
    (s) => s.task_id === taskId && s.kind.type === 'agent' && s.status !== 'exited',
  );
}

/** The session comments go to by default: the newest idle agent, else any live one; null means a new session. */
export function defaultTarget(sessions: Session[], taskId: number): number | null {
  const live = agentSessions(sessions, taskId).sort((a, b) => b.id - a.id);
  return (live.find((s) => s.status === 'idle') ?? live[0])?.id ?? null;
}

const LARGE_DIFF_LINES = 1000;
const MAX_OPEN_FILES = 30;

export const isLargeDiff = (f: FileDiff) => f.additions + f.deletions > LARGE_DIFF_LINES;

/** The diff view has no virtual scrolling, so big or late files start collapsed. */
export const startsCollapsed = (f: FileDiff, index: number) =>
  isLargeDiff(f) || index >= MAX_OPEN_FILES;

/** Forge timestamps are ISO strings, local ones unix seconds. */
export function formatTime(raw: string): string {
  const date = new Date(/^\d+$/.test(raw) ? Number(raw) * 1000 : raw);
  return isNaN(date.getTime()) ? raw : date.toLocaleString();
}

export type FileStatus = 'added' | 'deleted' | 'renamed' | 'modified';

/** Reads the status from the file's header lines (before the first hunk). */
export function fileStatus(patch: string): FileStatus {
  const hunk = patch.indexOf('\n@@');
  const header = hunk === -1 ? patch : patch.slice(0, hunk);
  if (/^new file mode/m.test(header)) return 'added';
  if (/^deleted file mode/m.test(header)) return 'deleted';
  if (/^rename to /m.test(header)) return 'renamed';
  return 'modified';
}

export function filterFiles(files: FileDiff[], query: string): FileDiff[] {
  const q = query.trim().toLowerCase();
  return q ? files.filter((f) => f.path.toLowerCase().includes(q)) : files;
}

export type TreeNode =
  | { kind: 'dir'; name: string; path: string; children: TreeNode[] }
  | { kind: 'file'; name: string; path: string; file: FileDiff };
type TreeDir = Extract<TreeNode, { kind: 'dir' }>;

function sortNodes(nodes: TreeNode[]): TreeNode[] {
  return nodes.sort((a, b) =>
    a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'dir' ? -1 : 1,
  );
}

function compact(node: TreeNode): TreeNode {
  if (node.kind === 'file') return node;
  let dir = node;
  while (dir.children.length === 1 && dir.children[0].kind === 'dir') {
    const only = dir.children[0];
    dir = {
      kind: 'dir',
      name: `${dir.name}/${only.name}`,
      path: only.path,
      children: only.children,
    };
  }
  return { ...dir, children: sortNodes(dir.children.map(compact)) };
}

/** A GitHub-like file tree; folder chains with a single child folder become one node. */
export function buildTree(files: FileDiff[]): TreeNode[] {
  const root: TreeDir = { kind: 'dir', name: '', path: '', children: [] };
  for (const file of files) {
    const parts = file.path.split('/');
    let dir = root;
    for (const part of parts.slice(0, -1)) {
      const path = dir.path ? `${dir.path}/${part}` : part;
      let next = dir.children.find((c): c is TreeDir => c.kind === 'dir' && c.name === part);
      if (!next) {
        next = { kind: 'dir', name: part, path, children: [] };
        dir.children.push(next);
      }
      dir = next;
    }
    dir.children.push({ kind: 'file', name: parts[parts.length - 1], path: file.path, file });
  }
  return sortNodes(root.children.map(compact));
}

export interface TreeRow {
  node: TreeNode;
  depth: number;
}

export function flattenTree(nodes: TreeNode[], collapsed: Set<string>, depth = 0): TreeRow[] {
  return nodes.flatMap((node) => [
    { node, depth },
    ...(node.kind === 'dir' && !collapsed.has(node.path)
      ? flattenTree(node.children, collapsed, depth + 1)
      : []),
  ]);
}

export function groupByDay(commits: ReviewCommit[]): { day: string; commits: ReviewCommit[] }[] {
  const groups: { day: string; commits: ReviewCommit[] }[] = [];
  for (const c of commits) {
    const day = c.date.slice(0, 10);
    if (groups[groups.length - 1]?.day === day) groups[groups.length - 1].commits.push(c);
    else groups.push({ day, commits: [c] });
  }
  return groups;
}

// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\[[0-9;?]*[ -/]*[@-~]/g;
const STAMP = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?Z ?/;

export const stripAnsi = (s: string) => s.replace(ANSI, '');

export interface LogSection {
  title: string | null;
  lines: string[];
  failed: boolean;
}

/** Splits a CI log into steps at `##[group]` markers; `##[endgroup]` lines are dropped. */
export function logSections(text: string): LogSection[] {
  const sections: LogSection[] = [];
  let current: LogSection = { title: null, lines: [], failed: false };
  for (const raw of stripAnsi(text).replace(/\n$/, '').split('\n')) {
    const line = raw.replace(STAMP, '');
    if (line.startsWith('##[group]')) {
      if (current.title !== null || current.lines.length) sections.push(current);
      current = { title: line.slice('##[group]'.length), lines: [], failed: false };
    } else if (!line.startsWith('##[endgroup]')) {
      if (line.startsWith('##[error]')) current.failed = true;
      current.lines.push(line);
    }
  }
  if (current.title !== null || current.lines.length) sections.push(current);
  return sections;
}

/** The failed sections (with their titles), else the whole log; at most the last `max` lines. */
export function logExcerpt(sections: LogSection[], max = 200): string {
  const failed = sections.filter((s) => s.failed);
  const lines = (failed.length ? failed : sections).flatMap((s) =>
    failed.length && s.title !== null ? [s.title, ...s.lines] : s.lines,
  );
  return lines.slice(-max).join('\n');
}

export function checkPrompt(name: string, excerpt: string): string {
  return `The CI check "${name}" failed. Log excerpt:\n\n\`\`\`\n${excerpt}\n\`\`\`\n\nFind and fix the cause.`;
}

export const failed = (c: CheckRun) =>
  c.status === 'done' &&
  ['failure', 'timed_out', 'cancelled', 'action_required', 'error'].includes(c.conclusion ?? '');

export function diffBar(additions: number, deletions: number): ('add' | 'del' | 'none')[] {
  const total = additions + deletions;
  const add = total ? Math.round((5 * additions) / total) : 0;
  const del = total ? Math.min(5 - add, Math.round((5 * deletions) / total)) : 0;
  return [
    ...Array<'add'>(add).fill('add'),
    ...Array<'del'>(del).fill('del'),
    ...Array<'none'>(5 - add - del).fill('none'),
  ];
}

export const pollInterval = (review: ReviewResult | null) =>
  review?.checks.some((c) => c.status !== 'done') ? 15_000 : 60_000;

/** Milliseconds for forge ISO times and local unix-epoch seconds alike. */
export function timeValue(raw: string): number {
  if (/^\d+$/.test(raw)) return Number(raw) * 1000;
  const t = Date.parse(raw);
  return Number.isNaN(t) ? 0 : t;
}

export interface ExcerptLine {
  tag: '+' | '-' | ' ';
  text: string;
}

/** Up to `before` diff lines before the commented line, plus that line, from one file's patch. */
export function codeExcerpt(
  filePatch: string,
  side: DiffSide,
  line: number,
  before = 3,
): ExcerptLine[] {
  const rows: (ExcerptLine & { old: number; new: number })[] = [];
  let [oldNo, newNo] = [0, 0];
  let inHunk = false;
  for (const l of filePatch.split('\n')) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(l);
    if (hunk) {
      [oldNo, newNo] = [Number(hunk[1]), Number(hunk[2])];
      inHunk = true;
      continue;
    }
    const tag = l[0];
    if (!inHunk || (tag !== '+' && tag !== '-' && tag !== ' ')) continue;
    rows.push({ tag, text: l.slice(1), old: oldNo, new: newNo });
    if (tag !== '+') oldNo++;
    if (tag !== '-') newNo++;
  }
  const i = rows.findIndex((r) =>
    side === 'new' ? r.tag !== '-' && r.new === line : r.tag !== '+' && r.old === line,
  );
  return i === -1
    ? []
    : rows.slice(Math.max(0, i - before), i + 1).map(({ tag, text }) => ({ tag, text }));
}

export type TimelineEntry =
  | { kind: 'item'; item: ConversationItem; at: number }
  | { kind: 'thread'; thread: ReviewThread; at: number };

export function timeline(review: ReviewResult): TimelineEntry[] {
  return [
    ...review.conversation.map((item) => ({
      kind: 'item' as const,
      item,
      at: timeValue(item.created_at),
    })),
    ...review.threads.map((thread) => ({
      kind: 'thread' as const,
      thread,
      at: timeValue(thread.comments[0]?.created_at ?? ''),
    })),
  ].sort((a, b) => a.at - b.at);
}

export type ReviewTab = 'conversation' | 'commits' | 'checks' | 'files';
/** The last open tab per task, for this app session. */
export const lastReviewTab = new Map<number, ReviewTab>();
