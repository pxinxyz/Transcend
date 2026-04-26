/**
 * Tests for the git_stash skill — arg rendering, normalization pipeline,
 * truncation, empty results, action-specific fields, and runtime integration.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { NunjucksEngine, TemplateRenderError } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';
import { NormalizationPipeline } from '../lib/NormalizationPipeline.js';
import { ExecutionResult } from '../lib/CommandExecutor.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering Tests ────────────────────────────────────

describe('git_stash arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders list action', () => {
    const spec = loader.load('git_stash');
    const args = engine.renderArgs(spec, { path: '.', action: 'list' });

    assert.ok(args.includes('stash'), 'includes stash subcommand');
    assert.ok(args.includes('list'), 'includes list action');
    assert.ok(args.some(a => a.includes('>>>STASH_START<<<')), 'includes structured format');
    assert.ok(args.includes('.'), 'includes path');
    assert.ok(!args.includes('--numstat'), 'no numstat for list');
  });

  it('renders push action with options', () => {
    const spec = loader.load('git_stash');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'push',
      message: 'WIP',
      include_untracked: true,
      keep_index: true,
      quiet: true
    });

    assert.ok(args.includes('push'), 'includes push action');
    assert.ok(args.includes('-m'), 'includes message flag');
    assert.ok(args.includes('WIP'), 'includes message value');
    assert.ok(args.includes('-u'), 'includes untracked flag');
    assert.ok(args.includes('-k'), 'includes keep-index flag');
    assert.ok(args.includes('-q'), 'includes quiet flag');
  });

  it('renders apply, pop, and drop action args', () => {
    const spec = loader.load('git_stash');

    const applyArgs = engine.renderArgs(spec, { path: '.', action: 'apply', stash_index: 2 });
    assert.ok(applyArgs.includes('apply'), 'includes apply');
    assert.ok(applyArgs.includes('stash@{2}'), 'includes stash ref with index 2');

    const popArgs = engine.renderArgs(spec, { path: '.', action: 'pop', stash_index: 1 });
    assert.ok(popArgs.includes('pop'), 'includes pop');
    assert.ok(popArgs.includes('stash@{1}'), 'includes stash ref with index 1');

    const dropArgs = engine.renderArgs(spec, { path: '.', action: 'drop', stash_index: 0 });
    assert.ok(dropArgs.includes('drop'), 'includes drop');
    assert.ok(dropArgs.includes('stash@{0}'), 'includes stash ref with index 0');
  });

  it('renders show, clear, and branch action args', () => {
    const spec = loader.load('git_stash');

    const showArgs = engine.renderArgs(spec, { path: '.', action: 'show', stash_index: 0 });
    assert.ok(showArgs.includes('show'), 'includes show');
    assert.ok(showArgs.includes('--numstat'), 'includes numstat by default');
    assert.ok(showArgs.includes('stash@{0}'), 'includes stash ref');

    const showPatchArgs = engine.renderArgs(spec, { path: '.', action: 'show', stash_index: 0, include_patch: true });
    assert.ok(showPatchArgs.includes('-p'), 'includes patch flag');
    assert.ok(!showPatchArgs.includes('--numstat'), 'no numstat when patch enabled');

    const clearArgs = engine.renderArgs(spec, { path: '.', action: 'clear' });
    assert.ok(clearArgs.includes('clear'), 'includes clear');

    const branchArgs = engine.renderArgs(spec, { path: '.', action: 'branch', branch_name: 'feature', stash_index: 0 });
    assert.ok(branchArgs.includes('branch'), 'includes branch');
    assert.ok(branchArgs.includes('feature'), 'includes branch name');
    assert.ok(branchArgs.includes('stash@{0}'), 'includes stash ref for branch');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_stash normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, args = ['stash', 'list']) {
    return new ExecutionResult({
      exitCode,
      signal: null,
      stdout,
      stderr: '',
      timedOut: false,
      command: 'git',
      args
    });
  }

  it('parses list output with stash@{N} format', () => {
    const stdout = `>>>STASH_START<<<
stash@{0}
abc123def456abc123def456abc123def456abc123
WIP on main: initial work
2024-01-15T09:30:00+00:00
3 days ago
>>>STASH_END<<<
>>>STASH_START<<<
stash@{1}
def789ghi012def789ghi012def789ghi012def789
On feature: experimental changes
2024-01-10T14:00:00+00:00
1 week ago
>>>STASH_END<<<`;

    const spec = loader.load('git_stash');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'list');
    assert.strictEqual(output.total_stashes, 2);
    assert.strictEqual(output.stashes[0].index, 0);
    assert.strictEqual(output.stashes[0].message, 'WIP on main: initial work');
    assert.strictEqual(output.stashes[0].hash, 'abc123def456abc123def456abc123def456abc123');
    assert.strictEqual(output.stashes[0].date, '2024-01-15T09:30:00+00:00');
    assert.strictEqual(output.stashes[0].date_relative, '3 days ago');
    assert.strictEqual(output.stashes[1].index, 1);
    assert.strictEqual(output.stashes[1].message, 'On feature: experimental changes');
  });

  it('parses show output with numstat lines', () => {
    const stdout = `3	1	src/index.js
5	2	src/utils.js
0	10	README.md`;

    const spec = loader.load('git_stash');
    const result = mockResult(stdout, 0, ['stash', 'show', '--numstat', 'stash@{0}']);
    const output = normalizer.run(spec, result, { action: 'show', stash_index: 0 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'show');
    assert.strictEqual(output.total_stashes, 1);
    const stash = output.stashes[0];
    assert.strictEqual(stash.index, 0);
    assert.strictEqual(stash.files.length, 3);
    assert.strictEqual(stash.files[0].path, 'src/index.js');
    assert.strictEqual(stash.files[0].additions, 3);
    assert.strictEqual(stash.files[0].deletions, 1);
    assert.strictEqual(stash.files[1].path, 'src/utils.js');
    assert.strictEqual(stash.files[1].additions, 5);
    assert.strictEqual(stash.files[1].deletions, 2);
    assert.strictEqual(stash.files[2].path, 'README.md');
    assert.strictEqual(stash.files[2].additions, 0);
    assert.strictEqual(stash.files[2].deletions, 10);
  });

  it('truncates stashes over max_results', () => {
    const stdout = `stash@{0}: First stash
stash@{1}: Second stash
stash@{2}: Third stash`;

    const spec = loader.load('git_stash');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list', max_results: 2 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_stashes, 2);
    assert.strictEqual(output.truncated, true);
    assert.strictEqual(output.stashes[0].index, 0);
    assert.strictEqual(output.stashes[1].index, 1);
  });

  it('handles empty stash list', () => {
    const stdout = '';
    const spec = loader.load('git_stash');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_stashes, 0);
    assert.deepStrictEqual(output.stashes, []);
    assert.strictEqual(output.truncated, false);
  });

  it('includes action-specific output fields', () => {
    const spec = loader.load('git_stash');

    const pushOutput = normalizer.run(spec, mockResult('Saved working directory...', 0, ['stash', 'push']), { action: 'push' });
    assert.strictEqual(pushOutput.created, true);
    assert.strictEqual(pushOutput.applied, undefined);

    const applyOutput = normalizer.run(spec, mockResult('', 0, ['stash', 'apply']), { action: 'apply' });
    assert.strictEqual(applyOutput.applied, true);

    const popOutput = normalizer.run(spec, mockResult('', 0, ['stash', 'pop']), { action: 'pop' });
    assert.strictEqual(popOutput.applied, true);
    assert.strictEqual(popOutput.dropped, true);

    const dropOutput = normalizer.run(spec, mockResult('', 0, ['stash', 'drop']), { action: 'drop' });
    assert.strictEqual(dropOutput.dropped, true);

    const clearOutput = normalizer.run(spec, mockResult('', 0, ['stash', 'clear']), { action: 'clear' });
    assert.strictEqual(clearOutput.cleared, true);

    const branchOutput = normalizer.run(spec, mockResult('', 0, ['stash', 'branch', 'feature']), { action: 'branch', branch_name: 'feature' });
    assert.strictEqual(branchOutput.branched, true);
    assert.strictEqual(branchOutput.branch_name, 'feature');
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_stash runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_stash'), 'git_stash is listed');
  });

  it('validates missing required action', () => {
    const engine = new NunjucksEngine();
    const spec = {
      inputs: {
        action: { type: 'string', required: true, enum: ['list', 'push', 'apply', 'pop', 'drop', 'show', 'clear', 'branch'] }
      },
      execution: {
        command: 'git',
        args: ['stash', '{{ action }}'],
        template_engine: 'nunjucks'
      }
    };

    assert.throws(
      () => engine.renderArgs(spec, {}),
      TemplateRenderError,
      'missing action throws'
    );
  });
});

console.log('All git_stash tests defined.');
