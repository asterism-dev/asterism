import type { PullRequest } from './types';

export type BadgeTone = 'draft' | 'pending' | 'success' | 'failure' | 'merged' | 'closed';

const STATE_LABELS: Record<PullRequest['state'], string> = { open: 'Open', draft: 'Draft', merged: 'Merged', closed: 'Closed' };
const REVIEW_LABELS: Record<PullRequest['review'], string | null> = {
  approved: 'approved', changes_requested: 'changes requested', review_required: 'review required', none: null,
};

function checksText(pr: PullRequest): string {
  const { state, failing } = pr.checks;
  if (state === 'failure') return `${failing.length} failing: ${failing.join(', ')}`;
  if (state === 'pending') return 'checks running';
  if (state === 'success') return 'checks passed';
  return 'no checks';
}

/** Review and checks in words, e.g. "changes requested · 2 failing: lint, test". */
export function prSummary(pr: PullRequest): string {
  return [REVIEW_LABELS[pr.review], checksText(pr)].filter(Boolean).join(' · ');
}

export function prBadge(pr: PullRequest): { tone: BadgeTone; label: string; tooltip: string } {
  let tone: BadgeTone;
  if (pr.state === 'merged' || pr.state === 'closed' || pr.state === 'draft') tone = pr.state;
  else if (pr.checks.state === 'failure' || pr.review === 'changes_requested') tone = 'failure';
  else if (pr.checks.state === 'pending') tone = 'pending';
  else tone = 'success';
  return { tone, label: `#${pr.number}`, tooltip: [STATE_LABELS[pr.state], prSummary(pr)].join(' · ') };
}
