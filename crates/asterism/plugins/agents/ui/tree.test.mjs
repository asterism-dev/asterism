import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildTree, formatDuration } from './tree.mjs';

const sub = (id, over = {}) => ({
  id, parent_id: null, kind: 'Explore', description: id, status: 'running', started_at: 1, ended_at: null, ...over,
});

test('nests subagents under parents and splits finished ones', () => {
  const [node] = buildTree([
    { id: 1, kind: { type: 'agent', name: 'claude' }, status: 'working',
      subagents: [sub('a'), sub('b', { status: 'done', ended_at: 5 }), sub('c', { parent_id: 'a' }), sub('d', { parent_id: 'gone' })] },
  ]);
  assert.equal(node.label, 'claude');
  assert.deepEqual(node.running.map((n) => n.id), ['a', 'd']);
  assert.deepEqual(node.running[0].children.map((n) => n.id), ['c']);
  assert.deepEqual(node.finished.map((n) => n.id), ['b']);
});

test('labels non-agent sessions and formats durations', () => {
  const [shell] = buildTree([{ id: 2, kind: { type: 'shell' }, status: 'idle', subagents: [] }]);
  assert.equal(shell.label, 'shell');
  assert.equal(formatDuration(5), '5s');
  assert.equal(formatDuration(125), '2m 05s');
  assert.equal(formatDuration(3720), '1h 02m');
});
