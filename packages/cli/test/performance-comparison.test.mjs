import test from 'node:test';
import assert from 'node:assert/strict';
import { comparePerformance } from '../../../scripts/compare-performance.mjs';

function recording(websiteMs = 1_000, appMs = 1_100) {
  const expected = [{ source: 'Sample', flow: 'reader', cache: 'cold' }];
  return { schemaVersion: 1, expected, samples: Array.from({ length: 5 }, (_, index) => ['website', 'app'].map(surface => ({
    ...expected[0], surface, pair: index + 1, durationMs: surface === 'website' ? websiteMs : appMs,
    status: 'ready', transport: 'live-public', clock: 'in-process-monotonic', start: 'navigation-request',
    end: 'frame-after-ready', milestone: 'first-readable-content', frameIntervalMs: 16.67,
    cacheState: 'isolated-empty', contentKey: 'same-chapter-and-first-page', machine: 'same-mac',
    network: 'same-network', viewport: { width: 1_032, height: 1_376 },
  }))).flat() };
}

test('uses the larger of 20 percent and 250 ms, with median and range', () => {
  const small = recording();
  small.samples[0].durationMs = 900;
  const result = comparePerformance(small)[0];
  assert.equal(result.verdict, 'pass');
  assert.equal(result.toleranceMs, 250);
  assert.deepEqual(result.website, { medianMs: 1_000, minimumMs: 900, maximumMs: 1_000, samples: 5 });
  assert.equal(comparePerformance(recording(2_000, 2_450))[0].verdict, 'fail');
  assert.equal(comparePerformance(recording(2_000, 2_450))[0].toleranceMs, 400);
});
test('does not turn frame uncertainty at the threshold into a pass or failure', () => {
  assert.equal(comparePerformance(recording(1_000, 1_250))[0].verdict, 'inconclusive');
});
test('missing expected flows and incomplete pairs remain unverified', () => {
  const input = recording();
  input.samples.pop();
  input.expected.push({ source: 'Blocked', flow: 'catalog', cache: 'warm' });
  assert.deepEqual(comparePerformance(input).map(x => x.verdict), ['unverified', 'unverified']);
});
test('rejects observer timings, failed readiness, fixtures and unknown cache conditions', () => {
  for (const [field, value] of [['clock', 'external-observer'], ['status', 'timeout'], ['transport', 'fixture'], ['cacheState', 'unknown']]) {
    const input = recording(); input.samples[0][field] = value;
    assert.equal(comparePerformance(input)[0].verdict, 'unverified', field);
  }
});
test('rejects mismatched content, viewport, milestones and machine', () => {
  for (const [field, value] of [['contentKey', 'another-chapter'], ['viewport', { width: 400, height: 800 }],
    ['milestone', 'reader-shell'], ['machine', 'different-mac']]) {
    const input = recording(); input.samples[0][field] = value;
    assert.equal(comparePerformance(input)[0].verdict, 'unverified', field);
  }
});
test('duplicates cannot silently overwrite failed attempts', () => {
  const input = recording(); input.samples.push({ ...input.samples[0], status: 'timeout' });
  assert.throws(() => comparePerformance(input), /Duplicate sample/);
});
test('five different matched titles cannot be pooled as repetitions of one flow', () => {
  const input = recording();
  input.samples[2].contentKey = input.samples[3].contentKey = 'different-title-in-second-pair';
  assert.equal(comparePerformance(input)[0].verdict, 'unverified');
});
test('validates warm caches independently from cold caches', () => {
  const input = recording(); input.expected[0].cache = 'warm';
  for (const sample of input.samples) sample.cache = 'warm';
  assert.equal(comparePerformance(input)[0].verdict, 'unverified');
  for (const sample of input.samples) sample.cacheState = 'retained-after-prime';
  assert.equal(comparePerformance(input)[0].verdict, 'pass');
});
test('invalid durations and an empty expected inventory cannot pass', () => {
  const input = recording(); input.samples[0].durationMs = NaN;
  assert.equal(comparePerformance(input)[0].verdict, 'unverified');
  assert.throws(() => comparePerformance({ schemaVersion: 1, expected: [], samples: [] }), /must not be empty/);
});
