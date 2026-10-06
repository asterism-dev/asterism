import type { IssueDetails, TaskIssue, TaskSourceInfo } from './types';

export interface SourceOption { id: string; label: string; plugin: string | null; disabled: boolean; hint: string | null }

export function sourceOptions(sources: TaskSourceInfo[]): SourceOption[] {
  return [
    { id: '', label: 'Blank', plugin: null, disabled: false, hint: null },
    ...sources.map((s) => ({ id: s.id, label: s.display_name, plugin: s.plugin, disabled: !s.available, hint: s.available ? null : s.reason })),
  ];
}

/** An empty query lists my open issues; text searches all open issues. */
export function searchParams(query: string): { query: string; assigned_to_me: boolean } {
  const trimmed = query.trim();
  return { query: trimmed, assigned_to_me: trimmed === '' };
}

export function issueToCreate(d: IssueDetails): TaskIssue {
  return { source: d.source, key: d.key, title: d.title, url: d.url, branch: d.branch || null };
}

/** Sends '' when the title is still the issue's name, so the daemon derives it (and handles collisions). */
export function createTitle(title: string, details: IssueDetails | null): string {
  const trimmed = title.trim();
  return details && trimmed === details.name ? '' : trimmed;
}
