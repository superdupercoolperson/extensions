import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const flows = new Set(['catalog', 'details', 'reader', 'next-page']);
const caches = new Set(['cold', 'warm']);
const surfaces = new Set(['website', 'app']);
const milestones = {
  catalog: 'visible-cards-and-decoded-covers',
  details: 'metadata-and-installments',
  reader: 'first-readable-content',
  'next-page': 'next-readable-content',
};

function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return {
    medianMs: sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2,
    minimumMs: sorted[0], maximumMs: sorted.at(-1), samples: sorted.length,
  };
}

/** Fail closed: external observer durations and fixture runs cannot establish live acceptance. */
export function comparePerformance(input) {
  if (input.schemaVersion !== 1 || !Array.isArray(input.samples) || !Array.isArray(input.expected)) {
    throw new Error('Expected schemaVersion 1, samples, and an explicit expected-flow inventory');
  }
  const key = ({ source, flow, cache }) => JSON.stringify([source, flow, cache]);
  const expected = new Map();
  for (const entry of input.expected) {
    if (!entry.source || !flows.has(entry.flow) || !caches.has(entry.cache) || expected.has(key(entry))) {
      throw new Error('Invalid or duplicate expected flow');
    }
    expected.set(key(entry), entry);
  }
  if (!expected.size) throw new Error('The expected inventory must not be empty');
  const groups = new Map();
  for (const sample of input.samples) {
    if (!expected.has(key(sample))) throw new Error('Sample is outside the expected inventory');
    if (!surfaces.has(sample.surface) || !Number.isInteger(sample.pair) || sample.pair < 1) {
      throw new Error('Invalid surface or pair');
    }
    const group = groups.get(key(sample)) ?? new Map();
    const pair = group.get(sample.pair) ?? {};
    if (pair[sample.surface]) throw new Error('Duplicate sample; do not silently replace failed attempts');
    pair[sample.surface] = sample;
    group.set(sample.pair, pair);
    groups.set(key(sample), group);
  }
  return [...expected].map(([id, expectedFlow]) => {
    const pairs = [...(groups.get(id) ?? new Map())];
    const reasons = [];
    if (pairs.length !== 5) reasons.push(`Expected five pairs; received ${pairs.length}`);
    const contexts = new Set(pairs.flatMap(([, pair]) => Object.values(pair)).map(sample => JSON.stringify([
      sample.contentKey, sample.machine, sample.network, sample.viewport?.width, sample.viewport?.height,
    ])));
    if (contexts.size > 1) reasons.push('Content or measurement context changed between samples');
    for (const [number, pair] of pairs) {
      if (!pair.website || !pair.app) { reasons.push(`Pair ${number}: missing surface`); continue; }
      for (const sample of Object.values(pair)) {
        const prefix = `Pair ${number} ${sample.surface}`;
        if (sample.status !== 'ready') reasons.push(`${prefix}: ${sample.status ?? 'missing status'}`);
        if (sample.transport !== 'live-public') reasons.push(`${prefix}: not public live transport`);
        if (sample.clock !== 'in-process-monotonic') reasons.push(`${prefix}: observer or unspecified clock`);
        if (sample.start !== 'navigation-request' || sample.end !== 'frame-after-ready'
            || sample.milestone !== milestones[sample.flow]) reasons.push(`${prefix}: unmatched timing endpoints`);
        if (!Number.isFinite(sample.durationMs) || sample.durationMs < 0) reasons.push(`${prefix}: invalid duration`);
        if (!Number.isFinite(sample.frameIntervalMs) || sample.frameIntervalMs <= 0 || sample.frameIntervalMs > 34) {
          reasons.push(`${prefix}: missing or excessive frame uncertainty`);
        }
        if (sample.cacheState !== (sample.cache === 'cold' ? 'isolated-empty' : 'retained-after-prime')) {
          reasons.push(`${prefix}: unverified cache state`);
        }
        if (!sample.contentKey || !sample.machine || !sample.network || !sample.viewport
            || !(sample.viewport.width > 0) || !(sample.viewport.height > 0)) reasons.push(`${prefix}: missing comparison context`);
      }
      for (const field of ['contentKey', 'machine', 'network']) {
        if (pair.website[field] !== pair.app[field]) reasons.push(`Pair ${number}: ${field} differs`);
      }
      if (pair.website.viewport?.width !== pair.app.viewport?.width
          || pair.website.viewport?.height !== pair.app.viewport?.height) reasons.push(`Pair ${number}: viewport differs`);
    }
    if (reasons.length) return { ...expectedFlow, verdict: 'unverified', reasons };
    const website = summary(pairs.map(([, p]) => p.website.durationMs));
    const app = summary(pairs.map(([, p]) => p.app.durationMs));
    const toleranceMs = Math.max(website.medianMs * 0.2, 250);
    const excessMs = app.medianMs - website.medianMs;
    // Two frame-aligned endpoints have bounded quantization. Near-boundary results need more precision.
    const uncertaintyMs = Math.max(...pairs.map(([, p]) => p.website.frameIntervalMs + p.app.frameIntervalMs));
    const verdict = excessMs - uncertaintyMs > toleranceMs ? 'fail'
      : excessMs + uncertaintyMs <= toleranceMs ? 'pass' : 'inconclusive';
    return { ...expectedFlow, verdict, website, app, toleranceMs, excessMs, uncertaintyMs };
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [inputPath, outputPath] = process.argv.slice(2);
  if (!inputPath) throw new Error('Usage: node scripts/compare-performance.mjs INPUT.json [OUTPUT.json]');
  const results = comparePerformance(JSON.parse(await readFile(inputPath, 'utf8')));
  const output = JSON.stringify({ schemaVersion: 1, results }, null, 2) + '\n';
  if (outputPath) await writeFile(outputPath, output);
  else process.stdout.write(output);
  if (results.some(result => result.verdict !== 'pass')) process.exitCode = 1;
}
