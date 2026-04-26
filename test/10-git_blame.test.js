/**
 * Tests for the git_blame skill — argument rendering, porcelain parsing,
 * commit caching, truncation, email mode, relative dates, empty output,
 * fallback envelope, and runtime integration.
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
import { ResilienceHandler } from '../lib/ResilienceHandler.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering Tests ────────────────────────────────────

describe('git_blame arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders file, line_start, line_end, and show_email', () => {
    const spec = loader.load('git_blame');
    const args = engine.renderArgs(spec, {
      file: 'src/index.js',
      line_start: 10,
      line_end: 20,
      show_email: true
    });

    assert.ok(args.includes('blame'), 'includes blame subcommand');
    assert.ok(args.includes('--porcelain'), 'includes porcelain flag');
    assert.ok(args.includes('src/index.js'), 'includes file path');
    assert.ok(args.some(a => a.startsWith('-L')), 'includes -L flag');
    assert.ok(args.some(a => a.includes('10')), 'includes line_start');
    assert.ok(args.some(a => a.includes('20')), 'includes line_end');
    // show_email is handled in normalization, not args
    assert.ok(!args.some(a => a.includes('show-email')), 'no show-email arg');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_blame normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0) {
    return new ExecutionResult({
      exitCode,
      signal: null,
      stdout,
      stderr: '',
      timedOut: false,
      command: 'git',
      args: ['blame', '--porcelain', 'test.js']
    });
  }

  it('parses porcelain output with commit caching', () => {
    // Lines 1-2 from commit A, line 3 from commit B
    const stdout = `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1 1 2
author Alice
author-mail <alice@example.com>
author-time 1700000000
author-tz +0000
summary First commit
filename test.js
	line one
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 2 2
	line two
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb 3 3 1
author Bob
author-mail <bob@example.com>
author-time 1700086400
author-tz +0000
summary Second commit
filename test.js
	line three`;

    const spec = loader.load('git_blame');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { file: 'test.js' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_lines, 3);
    assert.strictEqual(output.lines.length, 3);

    const first = output.lines[0];
    assert.strictEqual(first.line_number, 1);
    assert.strictEqual(first.commit_hash, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
    assert.strictEqual(first.short_commit, 'aaaaaaa');
    assert.strictEqual(first.author, 'Alice');
    assert.strictEqual(first.summary, 'First commit');
    assert.strictEqual(first.line_text, 'line one');

    const second = output.lines[1];
    assert.strictEqual(second.line_number, 2);
    assert.strictEqual(second.commit_hash, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
    assert.strictEqual(second.author, 'Alice');
    assert.strictEqual(second.line_text, 'line two');

    const third = output.lines[2];
    assert.strictEqual(third.line_number, 3);
    assert.strictEqual(third.commit_hash, 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
    assert.strictEqual(third.author, 'Bob');
    assert.strictEqual(third.line_text, 'line three');
  });

  it('truncates at max_lines', () => {
    const stdout = `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1 1 1
author Alice
author-mail <alice@example.com>
author-time 1700000000
author-tz +0000
summary First
filename test.js
	line one
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb 2 2 1
author Bob
author-mail <bob@example.com>
author-time 1700000001
author-tz +0000
summary Second
filename test.js
	line two
cccccccccccccccccccccccccccccccccccccccc 3 3 1
author Carol
author-mail <carol@example.com>
author-time 1700000002
author-tz +0000
summary Third
filename test.js
	line three`;

    const spec = loader.load('git_blame');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { file: 'test.js', max_lines: 2 });

    assert.strictEqual(output.total_lines, 2);
    assert.strictEqual(output.lines.length, 2);
    assert.strictEqual(output.truncated, true);
    assert.strictEqual(output.lines[0].line_text, 'line one');
    assert.strictEqual(output.lines[1].line_text, 'line two');
  });

  it('email mode returns author email when show_email=true', () => {
    const stdout = `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1 1 1
author Alice
author-mail <alice@example.com>
author-time 1700000000
author-tz +0000
summary First commit
filename test.js
	line one`;

    const spec = loader.load('git_blame');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { file: 'test.js', show_email: true });

    assert.strictEqual(output.lines[0].author, 'alice@example.com');
  });

  it('computes relative dates', () => {
    // Use a fixed "now" for deterministic relative date computation
    const originalNow = Date.now;
    Date.now = () => new Date('2024-01-15T12:00:00Z').getTime();

    try {
      const stdout = `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1 1 1
author Alice
author-mail <alice@example.com>
author-time 1705312800
author-tz +0000
summary First commit
filename test.js
	line one`;

      const spec = loader.load('git_blame');
      const result = mockResult(stdout);
      const output = normalizer.run(spec, result, { file: 'test.js' });

      assert.ok(output.lines[0].date, 'has absolute date');
      assert.ok(output.lines[0].date_relative, 'has relative date');
      assert.strictEqual(typeof output.lines[0].date_relative, 'string');
    } finally {
      Date.now = originalNow;
    }
  });

  it('handles empty output', () => {
    const stdout = '';
    const spec = loader.load('git_blame');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { file: 'test.js' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_lines, 0);
    assert.deepStrictEqual(output.lines, []);
    assert.strictEqual(output.truncated, false);
  });
});

// ── Fallback Envelope Test ─────────────────────────────────

describe('git_blame fallback envelope', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });

  it('annotates used_fallback via ResilienceHandler', () => {
    const handler = new ResilienceHandler();
    const spec = loader.load('git_blame');
    const envelope = {
      status: 'ok',
      file: 'test.js',
      total_lines: 1,
      lines: [],
      truncated: false,
      warnings: []
    };

    const result = handler.annotateFallback(envelope, spec);
    assert.strictEqual(result.used_fallback, true);
    assert.ok(Array.isArray(result.warnings));
    assert.ok(result.warnings.some(w => w.includes('Fallback mode')));
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_blame runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);
    assert.ok(names.includes('git_blame'), 'git_blame is listed');
  });

  it('executes git_blame on a real file', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_blame', {
      file: 'lib/NormalizationPipeline.js',
      max_lines: 5
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message}`);
    assert.ok(Array.isArray(result.lines), 'lines is array');
    assert.ok(result.total_lines > 0, 'has at least one line');
    assert.ok(result.total_lines <= 5, 'respects max_lines');
    assert.strictEqual(typeof result.file, 'string');

    const first = result.lines[0];
    assert.ok(first.commit_hash, 'line has commit_hash');
    assert.ok(first.short_commit, 'line has short_commit');
    assert.ok(first.author, 'line has author');
    assert.ok(first.date, 'line has date');
    assert.ok(first.date_relative, 'line has date_relative');
    assert.ok(first.summary, 'line has summary');
    assert.ok('line_text' in first, 'line has line_text');
    assert.strictEqual(first.line_number, 1, 'first line is line 1');
  });
});

console.log('All git_blame tests defined.');
