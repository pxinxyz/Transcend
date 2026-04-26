/**
 * Tests for the git_log skill — structured git commit history retrieval.
 * Covers arg rendering, schema validation, runtime integration, and
 * normalization pipeline parsing.
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

describe('git_log arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders basic git log with defaults', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {});

    assert.ok(args.includes('log'), 'includes log subcommand');
    assert.ok(args.some(a => a.includes('>>>COMMIT_START<<<')), 'structured format');
    assert.ok(args.includes('-n'), 'includes -n flag');
    assert.ok(args.includes('50'), 'default max_commits is 50');
    assert.ok(!args.includes('--oneline'), 'not oneline by default');
  });

  it('renders with max_commits and author', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, { max_commits: 10, author: 'pxin' });

    assert.ok(args.includes('-n'), 'includes -n');
    assert.ok(args.includes('10'), 'max_commits is 10');
    assert.ok(args.includes('--author=pxin'), 'author filter');
  });

  it('renders with since, until, and grep', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {
      since: '2024-01-01',
      until: '2024-12-31',
      grep: 'feat:'
    });

    assert.ok(args.includes('--since=2024-01-01'), 'since filter');
    assert.ok(args.includes('--until=2024-12-31'), 'until filter');
    assert.ok(args.includes('--grep=feat:'), 'grep filter');
  });

  it('renders with file and path', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {
      file: 'src/index.js',
      path: 'src'
    });

    assert.ok(args.includes('--'), 'includes -- separator');
    assert.ok(args.includes('src/index.js'), 'file path');
    assert.ok(args.includes('src'), 'directory path');
  });

  it('renders with include_stats and include_files', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {
      include_stats: true,
      include_files: true
    });

    assert.ok(args.includes('--stat'), 'stat flag');
    assert.ok(args.includes('--name-only'), 'name-only flag');
  });

  it('renders oneline format', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, { format: 'oneline' });

    assert.ok(args.includes('--oneline'), 'oneline flag');
    assert.ok(!args.some(a => a.includes('>>>COMMIT_START<<<')), 'no structured format');
  });

  it('renders all_branches and no_merges', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {
      all_branches: true,
      no_merges: true
    });

    assert.ok(args.includes('--all'), 'all branches flag');
    assert.ok(args.includes('--no-merges'), 'no merges flag');
  });

  it('omits conditional flags when false', () => {
    const spec = loader.load('git_log');
    const args = engine.renderArgs(spec, {
      include_stats: false,
      include_files: false,
      all_branches: false,
      no_merges: false
    });

    assert.ok(!args.includes('--stat'), 'no stat flag');
    assert.ok(!args.includes('--name-only'), 'no name-only flag');
    assert.ok(!args.includes('--all'), 'no all flag');
    assert.ok(!args.includes('--no-merges'), 'no no-merges flag');
  });
});

// ── Structural Integrity Tests ─────────────────────────────

describe('git_log structural integrity', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });

  it('validates skill spec structure', () => {
    const spec = loader.load('git_log');

    assert.strictEqual(spec.name, 'git_log');
    assert.strictEqual(spec.version, '1.0.0');
    assert.strictEqual(spec.stability, 'stable');
    assert.strictEqual(spec.execution.command, 'git');
    assert.ok(Array.isArray(spec.execution.args));
    assert.ok(spec.normalization.pipeline.length > 0);
    assert.ok(spec.chains.compatible_downstream.length > 0);
  });

  it('validates max_commits type and range', () => {
    const engine = new NunjucksEngine();
    const spec = loader.load('git_log');

    assert.throws(
      () => engine.renderArgs(spec, { max_commits: 'ten' }),
      TemplateRenderError,
      'max_commits must be integer'
    );
    assert.throws(
      () => engine.renderArgs(spec, { max_commits: 0 }),
      TemplateRenderError,
      'max_commits must be >= 1'
    );
    assert.throws(
      () => engine.renderArgs(spec, { max_commits: 1001 }),
      TemplateRenderError,
      'max_commits must be <= 1000'
    );
  });

  it('validates format enum', () => {
    const engine = new NunjucksEngine();
    const spec = loader.load('git_log');

    assert.throws(
      () => engine.renderArgs(spec, { format: 'json' }),
      TemplateRenderError,
      'format must be enum value'
    );

    // Valid enum values should not throw
    const args1 = engine.renderArgs(spec, { format: 'structured' });
    assert.ok(args1);
    const args2 = engine.renderArgs(spec, { format: 'oneline' });
    assert.ok(args2);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_log runtime integration', () => {
  it('lists git_log skill', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);
    assert.ok(names.includes('git_log'), 'runtime lists git_log');
  });

  it('executes git_log on this repo', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_log', {
      max_commits: 5,
      path: '.'
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message}`);
    assert.ok(Array.isArray(result.commits), 'commits is array');
    assert.ok(result.total_commits > 0, 'has at least one commit');
    assert.ok(result.total_commits <= 5, 'respects max_commits');

    const first = result.commits[0];
    assert.ok(first.hash, 'commit has hash');
    assert.ok(first.short_hash, 'commit has short_hash');
    assert.ok(first.subject, 'commit has subject');
    assert.ok('body' in first, 'commit has body');
    assert.ok(first.author, 'commit has author');
    assert.ok(first.committer, 'commit has committer');
    assert.ok(first.date, 'commit has date');
    assert.ok(first.date_relative, 'commit has date_relative');
  });

  it('validates invalid max_commits at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_log', {
      max_commits: -1
    });

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
  });

  it('validates invalid format at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_log', {
      format: 'xml'
    });

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_log normalization pipeline', () => {
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
      args: ['log']
    });
  }

  it('parses single commit with full metadata', () => {
    const stdout = `>>>COMMIT_START<<<
abc123def456abc123def456abc123def456abc123
abc1234
Initial commit

>>>META<<<
Alice
Bob
2024-01-15T09:30:00+00:00
3 days ago
>>>COMMIT_END<<<`;

    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_commits, 1);
    const commit = output.commits[0];
    assert.strictEqual(commit.hash, 'abc123def456abc123def456abc123def456abc123');
    assert.strictEqual(commit.short_hash, 'abc1234');
    assert.strictEqual(commit.subject, 'Initial commit');
    assert.strictEqual(commit.body, '');
    assert.strictEqual(commit.author, 'Alice');
    assert.strictEqual(commit.committer, 'Bob');
    assert.strictEqual(commit.date, '2024-01-15T09:30:00+00:00');
    assert.strictEqual(commit.date_relative, '3 days ago');
  });

  it('parses commit with empty body', () => {
    const stdout = `>>>COMMIT_START<<<
def789ghi012def789ghi012def789ghi012def789
def7890
Fix typo

>>>META<<<
Charlie
Charlie
2024-02-01T12:00:00+00:00
2 weeks ago
>>>COMMIT_END<<<`;

    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.total_commits, 1);
    assert.strictEqual(output.commits[0].subject, 'Fix typo');
    assert.strictEqual(output.commits[0].body, '');
  });

  it('parses commit with multi-line body', () => {
    const stdout = `>>>COMMIT_START<<<
1111111111111111111111111111111111111111
1111111
Add feature X

This adds the long-awaited feature.
It includes tests and documentation.

Closes #42.
>>>META<<<
Dana
Dana
2024-03-10T15:45:00+00:00
1 month ago
>>>COMMIT_END<<<`;

    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.total_commits, 1);
    assert.strictEqual(output.commits[0].subject, 'Add feature X');
    assert.ok(output.commits[0].body.includes('long-awaited feature'));
    assert.ok(output.commits[0].body.includes('Closes #42.'));
  });

  it('parses commit with stats', () => {
    const stdout = `>>>COMMIT_START<<<
2222222222222222222222222222222222222222
2222222
Update readme

>>>META<<<
Eve
Eve
2024-04-20T08:00:00+00:00
5 days ago
>>>COMMIT_END<<<
 README.md | 10 ++++++++++
 1 file changed, 10 insertions(+)
`;

    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { include_stats: true });

    assert.strictEqual(output.total_commits, 1);
    const commit = output.commits[0];
    assert.ok(commit.stats, 'has stats');
    assert.strictEqual(commit.stats.files, 1);
    assert.strictEqual(commit.stats.insertions, 10);
    assert.strictEqual(commit.stats.deletions, 0);
  });

  it('parses commit with files_changed', () => {
    const stdout = `>>>COMMIT_START<<<
3333333333333333333333333333333333333333
3333333
Refactor utils

>>>META<<<
Frank
Frank
2024-05-01T10:00:00+00:00
1 day ago
>>>COMMIT_END<<<
src/utils.js
src/helpers.js
`;

    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { include_files: true });

    assert.strictEqual(output.total_commits, 1);
    const commit = output.commits[0];
    assert.ok(Array.isArray(commit.files_changed), 'has files_changed array');
    assert.strictEqual(commit.files_changed.length, 2);
    assert.ok(commit.files_changed.includes('src/utils.js'));
    assert.ok(commit.files_changed.includes('src/helpers.js'));
  });

  it('handles empty git log output', () => {
    const stdout = '';
    const spec = loader.load('git_log');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_commits, 0);
    assert.deepStrictEqual(output.commits, []);
    assert.strictEqual(output.truncated, false);
  });
});

console.log('All git_log tests defined.');
