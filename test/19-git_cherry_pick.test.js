/**
 * Tests for the git_cherry_pick skill — arg rendering, normalization pipeline,
 * conflict detection, in-progress state, fallback envelope, and runtime integration.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { mkdirSync, rmSync, existsSync } from 'fs';
import { NunjucksEngine, TemplateRenderError } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';
import { NormalizationPipeline } from '../lib/NormalizationPipeline.js';
import { ExecutionResult } from '../lib/CommandExecutor.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering Tests ────────────────────────────────────

describe('git_cherry_pick arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders pick action with options', () => {
    const spec = loader.load('git_cherry_pick');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'pick',
      commits: 'abc1234 def5678',
      strategy: 'ort',
      no_commit: true,
      signoff: true,
      mainline: 1,
      edit: false
    });

    assert.ok(args.includes('cherry-pick'), 'includes cherry-pick subcommand');
    assert.ok(args.includes('abc1234'), 'includes first commit');
    assert.ok(args.includes('def5678'), 'includes second commit');
    assert.ok(args.includes('--strategy=ort'), 'includes strategy');
    assert.ok(args.includes('--no-commit'), 'includes no-commit');
    assert.ok(args.includes('--signoff'), 'includes signoff');
    assert.ok(args.includes('--mainline=1'), 'includes mainline');
    assert.ok(args.includes('--no-edit'), 'includes no-edit');
    assert.ok(!args.includes('--abort'), 'no abort flag');
  });

  it('renders continue action', () => {
    const spec = loader.load('git_cherry_pick');
    const args = engine.renderArgs(spec, { path: '.', action: 'continue' });

    assert.ok(args.includes('cherry-pick'), 'includes cherry-pick');
    assert.ok(args.includes('--continue'), 'includes --continue');
  });

  it('renders abort action', () => {
    const spec = loader.load('git_cherry_pick');
    const args = engine.renderArgs(spec, { path: '.', action: 'abort' });

    assert.ok(args.includes('cherry-pick'), 'includes cherry-pick');
    assert.ok(args.includes('--abort'), 'includes --abort');
  });

  it('renders quit action', () => {
    const spec = loader.load('git_cherry_pick');
    const args = engine.renderArgs(spec, { path: '.', action: 'quit' });

    assert.ok(args.includes('cherry-pick'), 'includes cherry-pick');
    assert.ok(args.includes('--quit'), 'includes --quit');
  });

  it('renders skip action', () => {
    const spec = loader.load('git_cherry_pick');
    const args = engine.renderArgs(spec, { path: '.', action: 'skip' });

    assert.ok(args.includes('cherry-pick'), 'includes cherry-pick');
    assert.ok(args.includes('--skip'), 'includes --skip');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_cherry_pick normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, stderr = '', args = ['cherry-pick']) {
    return new ExecutionResult({
      exitCode,
      signal: null,
      stdout,
      stderr,
      timedOut: false,
      command: 'git',
      args
    });
  }

  it('parses successful pick output', () => {
    const stdout = `[main abc1234] Fix bug in parser
[main def5678] Update documentation`;

    const spec = loader.load('git_cherry_pick');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'pick' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'pick');
    assert.strictEqual(output.total_picked, 2);
    assert.strictEqual(output.picked[0].commit, 'abc1234');
    assert.strictEqual(output.picked[0].short_commit, 'abc1234');
    assert.strictEqual(output.picked[0].subject, 'Fix bug in parser');
    assert.strictEqual(output.picked[1].commit, 'def5678');
    assert.strictEqual(output.picked[1].subject, 'Update documentation');
    assert.strictEqual(output.has_conflicts, false);
    assert.strictEqual(output.in_progress, false);
    assert.deepStrictEqual(output.conflicts, []);
  });

  it('detects conflicts from stderr', () => {
    const stdout = '';
    const stderr = `error: could not apply abc1234... Fix bug in parser
CONFLICT (content): Merge conflict in src/parser.js
CONFLICT (modify/delete): src/old.js deleted in HEAD and modified in abc1234... Fix bug in parser
hint: after resolving the conflicts, mark them with`;

    const spec = loader.load('git_cherry_pick');
    const result = mockResult(stdout, 1, stderr);
    const output = normalizer.run(spec, result, { action: 'pick' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.has_conflicts, true);
    assert.strictEqual(output.conflicts.length, 2);
    assert.strictEqual(output.conflicts[0].file, 'src/parser.js');
    assert.strictEqual(output.conflicts[0].type, 'content');
    assert.strictEqual(output.conflicts[1].file, 'src/old.js');
    assert.strictEqual(output.conflicts[1].type, 'modify/delete');
    assert.strictEqual(output.in_progress, true);
  });

  it('detects in-progress state from .git/sequencer', () => {
    const tmpDir = join(__dirname, '..', 'tmp_cherry_pick_seq');
    const sequencerDir = join(tmpDir, '.git', 'sequencer');

    // Setup
    if (!existsSync(sequencerDir)) {
      mkdirSync(sequencerDir, { recursive: true });
    }

    try {
      const spec = loader.load('git_cherry_pick');
      const result = mockResult('');
      const output = normalizer.run(spec, result, { action: 'pick', path: tmpDir });

      assert.strictEqual(output.status, 'ok');
      assert.strictEqual(output.in_progress, true);
    } finally {
      // Cleanup
      if (existsSync(tmpDir)) {
        rmSync(tmpDir, { recursive: true, force: true });
      }
    }
  });

  it('handles abort/quit/skip actions', () => {
    const spec = loader.load('git_cherry_pick');

    const abortOutput = normalizer.run(spec, mockResult('', 0, '', ['cherry-pick', '--abort']), { action: 'abort' });
    assert.strictEqual(abortOutput.status, 'ok');
    assert.strictEqual(abortOutput.action, 'abort');
    assert.strictEqual(abortOutput.total_picked, 0);

    const quitOutput = normalizer.run(spec, mockResult('', 0, '', ['cherry-pick', '--quit']), { action: 'quit' });
    assert.strictEqual(quitOutput.status, 'ok');
    assert.strictEqual(quitOutput.action, 'quit');

    const skipOutput = normalizer.run(spec, mockResult('', 0, '', ['cherry-pick', '--skip']), { action: 'skip' });
    assert.strictEqual(skipOutput.status, 'ok');
    assert.strictEqual(skipOutput.action, 'skip');

    const continueOutput = normalizer.run(spec, mockResult('', 0, '', ['cherry-pick', '--continue']), { action: 'continue' });
    assert.strictEqual(continueOutput.status, 'ok');
    assert.strictEqual(continueOutput.action, 'continue');
  });

  it('handles empty output', () => {
    const spec = loader.load('git_cherry_pick');
    const result = mockResult('');
    const output = normalizer.run(spec, result, { action: 'pick' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_picked, 0);
    assert.deepStrictEqual(output.picked, []);
    assert.deepStrictEqual(output.conflicts, []);
    assert.strictEqual(output.has_conflicts, false);
    assert.strictEqual(output.in_progress, false);
    assert.strictEqual(output.truncated, false);
  });

  it('produces fallback envelope via git_cherry_pick_short', () => {
    const spec = {
      name: 'git_cherry_pick',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'parse_git_cherry_pick' },
          { step: 'git_cherry_pick_short' },
          { step: 'assemble_output' }
        ]
      }
    };
    // Use plain hash+subject format so primary parser misses it and fallback runs
    const result = mockResult('a1b2c3d Fallback commit message');
    const output = normalizer.run(spec, result, { action: 'pick' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_picked, 1);
    assert.strictEqual(output.picked[0].commit, 'a1b2c3d');
    assert.strictEqual(output.picked[0].subject, 'Fallback commit message');
    assert.strictEqual(output.used_fallback, true);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_cherry_pick runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_cherry_pick'), 'git_cherry_pick is listed');
  });
});

console.log('All git_cherry_pick tests defined.');
