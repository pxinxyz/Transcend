/**
 * Tests for PipelineProfiler — skill execution profiling.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { PipelineProfiler } from '../lib/PipelineProfiler.js';

describe('PipelineProfiler', () => {
  it('records a skill execution', () => {
    const profiler = new PipelineProfiler();
    const entry = profiler.record('git_status', 1000, 1050, {
      normalizationSteps: [
        { step: 'parse_git_status', duration_ms: 5 },
        { step: 'compute_summary', duration_ms: 3 }
      ]
    });

    assert.strictEqual(entry.skillName, 'git_status');
    assert.strictEqual(entry.duration, 50);
    assert.strictEqual(entry.normalizationSteps.length, 2);
  });

  it('returns all history', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 10);
    profiler.record('b', 10, 30);

    const history = profiler.getHistory();
    assert.strictEqual(history.length, 2);
    assert.strictEqual(history[0].skillName, 'a');
    assert.strictEqual(history[1].skillName, 'b');
  });

  it('returns filtered history by skill name', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 10);
    profiler.record('b', 10, 30);
    profiler.record('a', 30, 40);

    const history = profiler.getHistory('a');
    assert.strictEqual(history.length, 2);
    assert.ok(history.every(h => h.skillName === 'a'));
  });

  it('computes overall averages', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 100);
    profiler.record('b', 100, 250);

    const avg = profiler.getAverages();
    assert.strictEqual(avg.count, 2);
    assert.strictEqual(avg.avgDuration, 125);
    assert.strictEqual(avg.avgNormalizationTime, 0);
  });

  it('computes per-skill averages', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 100);
    profiler.record('a', 100, 300);
    profiler.record('b', 300, 400);

    const avg = profiler.getAverages('a');
    assert.strictEqual(avg.count, 2);
    assert.strictEqual(avg.avgDuration, 150);
  });

  it('returns zero averages when empty', () => {
    const profiler = new PipelineProfiler();
    const avg = profiler.getAverages();
    assert.strictEqual(avg.count, 0);
    assert.strictEqual(avg.avgDuration, 0);
    assert.strictEqual(avg.avgNormalizationTime, 0);
  });

  it('reports insufficient data for trends with few entries', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 10);
    profiler.record('a', 10, 20);
    profiler.record('a', 20, 30);

    const trends = profiler.getTrends('a');
    assert.strictEqual(trends.durationTrend, 'insufficient_data');
    assert.strictEqual(trends.normalizationTrend, 'insufficient_data');
  });

  it('detects improving trend', () => {
    const profiler = new PipelineProfiler();
    // Older: slow
    for (let i = 0; i < 4; i++) {
      profiler.record('a', i * 100, i * 100 + 100);
    }
    // Recent: fast
    for (let i = 4; i < 8; i++) {
      profiler.record('a', i * 100, i * 100 + 10);
    }

    const trends = profiler.getTrends('a');
    assert.strictEqual(trends.durationTrend, 'improving');
  });

  it('detects degrading trend', () => {
    const profiler = new PipelineProfiler();
    // Older: fast
    for (let i = 0; i < 4; i++) {
      profiler.record('a', i * 100, i * 100 + 10);
    }
    // Recent: slow
    for (let i = 4; i < 8; i++) {
      profiler.record('a', i * 100, i * 100 + 100);
    }

    const trends = profiler.getTrends('a');
    assert.strictEqual(trends.durationTrend, 'degrading');
  });

  it('detects stable trend', () => {
    const profiler = new PipelineProfiler();
    // All roughly the same
    for (let i = 0; i < 8; i++) {
      profiler.record('a', i * 100, i * 100 + 50);
    }

    const trends = profiler.getTrends('a');
    assert.strictEqual(trends.durationTrend, 'stable');
  });

  it('clears all history', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 10);
    profiler.clear();

    assert.strictEqual(profiler.getHistory().length, 0);
    assert.strictEqual(profiler.count, 0);
  });

  it('tracks normalization step timing', () => {
    const profiler = new PipelineProfiler();
    profiler.record('a', 0, 100, {
      normalizationSteps: [
        { step: 'step1', duration_ms: 10 },
        { step: 'step2', duration_ms: 20 }
      ]
    });

    const avg = profiler.getAverages('a');
    assert.strictEqual(avg.avgNormalizationTime, 30);
  });

  it('reports duration >= 0', () => {
    const profiler = new PipelineProfiler();
    const now = Date.now();
    const entry = profiler.record('a', now, now);
    assert.strictEqual(entry.duration, 0);
  });
});

console.log('All PipelineProfiler tests defined.');
