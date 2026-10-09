import type { CommentTarget, ReviewSource, ReviewThread, Session } from './types';

export interface FileDiff {
  path: string;
  patch: string;
  additions: number;
  deletions: number;
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
      return { path: patchPath(header), patch: chunk, additions, deletions };
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
