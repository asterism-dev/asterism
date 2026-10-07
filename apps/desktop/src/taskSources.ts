import type { IssueDetails, TaskIssue } from './types';

/** An empty query lists my open issues; text searches all open issues. */
export function searchParams(query: string): { query: string; assigned_to_me: boolean } {
  const trimmed = query.trim();
  return { query: trimmed, assigned_to_me: trimmed === '' };
}

export function issueToCreate(d: IssueDetails): TaskIssue {
  return { source: d.source, key: d.key, title: d.title, url: d.url, branch: d.branch || null };
}
