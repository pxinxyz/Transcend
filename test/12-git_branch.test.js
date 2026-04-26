/**
 * Tests for the git_branch skill — branch listing, creation, deletion,
 * renaming, and normalization pipeline parsing.
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

describe('git_branch arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders list action with defaults', () => {
    const spec = loader.load('git_branch');
    const args = engine.renderArgs(spec, {});

    assert.ok(args.includes('branch'), 'includes branch subcommand');
    assert.ok(args.includes('-vv'), 'includes -vv for verbose by default');
    assert.ok(args.includes('.'), 'default path is .');
    assert.ok(!args.includes('-a'), 'no -a by default');
    assert.ok(!args.includes('-r'), 'no -r by default');
  });

  it('renders list action with all and remote flags', () => {
    const spec = loader.load('git_branch');
    const args = engine.renderArgs(spec, { all: true, remote: true });

    assert.ok(args.includes('-a'), 'includes -a flag');
    assert.ok(args.includes('-r'), 'includes -r flag');
  });

  it('renders create, delete, rename action args', () => {
    const spec = loader.load('git_branch');

    const createArgs = engine.renderArgs(spec, {
      action: 'create',
      branch_name: 'feature-x',
      start_point: 'main'
    });
    assert.ok(!createArgs.includes('-vv'), 'no -vv for create');
    assert.ok(createArgs.includes('feature-x'), 'includes branch name');
    assert.ok(createArgs.includes('main'), 'includes start_point');

    const deleteArgs = engine.renderArgs(spec, {
      action: 'delete',
      branch_name: 'old-branch'
    });
    assert.ok(deleteArgs.includes('-d'), 'includes -d for delete');
    assert.ok(deleteArgs.includes('old-branch'), 'includes branch to delete');

    const forceArgs = engine.renderArgs(spec, {
      action: 'force_delete',
      branch_name: 'stale-branch'
    });
    assert.ok(forceArgs.includes('-D'), 'includes -D for force delete');

    const renameArgs = engine.renderArgs(spec, {
      action: 'rename',
      branch_name: 'old-name',
      new_branch_name: 'new-name'
    });
    assert.ok(renameArgs.includes('-m'), 'includes -m for rename');
    assert.ok(renameArgs.includes('old-name'), 'includes old name');
    assert.ok(renameArgs.includes('new-name'), 'includes new name');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_branch normalization pipeline', () => {
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
      args: ['branch', '-vv']
    });
  }

  it('parses verbose branch output with upstream tracking', () => {
    const stdout = `* main                a1b2c3d [origin/main] Latest commit on main
  feature-x           e4f5c6d [origin/feature-x: ahead 2, behind 1] WIP feature
  docs                9ab9012 [origin/docs: ahead 5] Update documentation
`;

    const spec = loader.load('git_branch');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_branches, 3);
    assert.strictEqual(output.current_branch, 'main');

    const main = output.branches.find(b => b.name === 'main');
    assert.ok(main, 'main branch exists');
    assert.strictEqual(main.current, true);
    assert.strictEqual(main.commit, 'a1b2c3d');
    assert.strictEqual(main.subject, 'Latest commit on main');
    assert.strictEqual(main.remote, 'origin/main');
    assert.strictEqual(main.ahead, undefined);
    assert.strictEqual(main.behind, undefined);

    const feature = output.branches.find(b => b.name === 'feature-x');
    assert.ok(feature, 'feature-x branch exists');
    assert.strictEqual(feature.current, false);
    assert.strictEqual(feature.commit, 'e4f5c6d');
    assert.strictEqual(feature.remote, 'origin/feature-x');
    assert.strictEqual(feature.ahead, 2);
    assert.strictEqual(feature.behind, 1);

    const docs = output.branches.find(b => b.name === 'docs');
    assert.ok(docs, 'docs branch exists');
    assert.strictEqual(docs.ahead, 5);
    assert.strictEqual(docs.behind, undefined);
  });

  it('parses detached HEAD and remote-tracking branches', () => {
    const stdout = `* (HEAD detached at v1.2.3)  abc1234 Some commit at tag
  remotes/origin/main          abc1234 Some commit at tag
  remotes/origin/feature       def5678 Feature work
`;

    const spec = loader.load('git_branch');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_branches, 3);

    const detached = output.branches.find(b => b.name === '(HEAD detached at v1.2.3)');
    assert.ok(detached, 'detached HEAD parsed');
    assert.strictEqual(detached.current, true);
    assert.strictEqual(detached.commit, 'abc1234');
    assert.strictEqual(detached.subject, 'Some commit at tag');

    const remote = output.branches.find(b => b.name === 'remotes/origin/feature');
    assert.ok(remote, 'remote-tracking branch parsed');
    assert.strictEqual(remote.current, false);
    assert.strictEqual(remote.commit, 'def5678');
    assert.strictEqual(remote.subject, 'Feature work');

    // current_branch should be the detached HEAD identifier
    assert.strictEqual(output.current_branch, '(HEAD detached at v1.2.3)');
  });

  it('truncates branches exceeding max_results', () => {
    const stdout = `  b1                  a111111 Commit 1
  b2                  a222222 Commit 2
  b3                  a333333 Commit 3
  b4                  a444444 Commit 4
`;

    const spec = loader.load('git_branch');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { max_results: 2 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_branches, 2);
    assert.strictEqual(output.truncated, true);
    assert.strictEqual(output.branches.length, 2);
  });

  it('handles empty branch output', () => {
    const stdout = '';
    const spec = loader.load('git_branch');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_branches, 0);
    assert.deepStrictEqual(output.branches, []);
    assert.strictEqual(output.truncated, false);
  });
});

// ── Runtime Integration Test ───────────────────────────────

describe('git_branch runtime integration', () => {
  it('integrates with runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_branch'), 'runtime lists git_branch');

    const result = await rt.execute('git_branch', {
      action: 'list',
      path: '.'
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message || JSON.stringify(result)}`);
    assert.ok(Array.isArray(result.branches), 'branches is array');
    assert.ok(result.total_branches > 0, 'has at least one branch');

    const first = result.branches[0];
    assert.ok(first.name, 'branch has name');
    assert.ok(first.commit, 'branch has commit');
    assert.ok(first.subject, 'branch has subject');
    assert.strictEqual(typeof first.current, 'boolean', 'branch has current boolean');
  });
});

console.log('All git_branch tests defined.');
