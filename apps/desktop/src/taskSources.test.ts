import { describe, expect, it } from 'vitest';
import { issueToCreate, searchParams } from './taskSources';
import type { IssueDetails } from './types';

describe('taskSources', () => {
  it('lists my issues when the query is empty and searches all otherwise', () => {
    expect(searchParams('  ')).toEqual({ query: '', assigned_to_me: true });
    expect(searchParams(' login ')).toEqual({ query: 'login', assigned_to_me: false });
  });

  it('passes the derived branch to task creation', () => {
    const details: IssueDetails = {
      source: 'linear', key: 'TRA-1', title: 'Fix', url: 'https://linear.app/x/issue/TRA-1', description: '',
      name: 'tra-1-fix', branch: 'feature/tra-1-fix', prompt: '# Fix',
    };
    expect(issueToCreate(details)).toEqual({ source: 'linear', key: 'TRA-1', title: 'Fix', url: 'https://linear.app/x/issue/TRA-1', branch: 'feature/tra-1-fix' });
  });

  it('sends no branch when the source has none', () => {
    const details: IssueDetails = {
      source: 'linear', key: 'TRA-1', title: 'Fix', url: 'u', description: '', name: 'tra-1-fix', branch: '', prompt: '# Fix',
    };
    expect(issueToCreate(details).branch).toBeNull();
  });
});
