/**
 * Tests for the git_remote skill — arg rendering, normalization pipeline,
 * fallback envelope, and runtime integration.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { NunjucksEngine } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';
import { NormalizationPipeline } from '../lib/NormalizationPipeline.js';
import { ExecutionResult } from '../lib/CommandExecutor.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering Tests ────────────────────────────────────

describe('git_remote arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders list action args', () => {
    const spec = loader.load('git_remote');
    const args = engine.renderArgs(spec, { path: '.', action: 'list' });

    assert.ok(args.includes('remote'), 'includes remote subcommand');
    assert.ok(args.includes('-v'), 'includes -v for list');
    assert.ok(args.includes('.'), 'includes path');
  });

  it('renders add and remove action args', () => {
    const spec = loader.load('git_remote');

    const addArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'add',
      remote_name: 'upstream',
      url: 'https://github.com/upstream/repo.git',
      branch: 'main'
    });
    assert.ok(addArgs.includes('add'), 'includes add');
    assert.ok(addArgs.includes('-t'), 'includes -t for branch tracking');
    assert.ok(addArgs.includes('main'), 'includes branch name');
    assert.ok(addArgs.includes('upstream'), 'includes remote name');
    assert.ok(addArgs.includes('https://github.com/upstream/repo.git'), 'includes url');

    const removeArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'remove',
      remote_name: 'origin'
    });
    assert.ok(removeArgs.includes('remove'), 'includes remove');
    assert.ok(removeArgs.includes('origin'), 'includes remote name');
  });

  it('renders rename and set_url action args', () => {
    const spec = loader.load('git_remote');

    const renameArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'rename',
      remote_name: 'origin',
      new_name: 'upstream'
    });
    assert.ok(renameArgs.includes('rename'), 'includes rename');
    assert.ok(renameArgs.includes('origin'), 'includes old name');
    assert.ok(renameArgs.includes('upstream'), 'includes new name');

    const setUrlArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'set_url',
      remote_name: 'origin',
      url: 'https://new-url.git',
      push_url: true
    });
    assert.ok(setUrlArgs.includes('set-url'), 'includes set-url');
    assert.ok(setUrlArgs.includes('--push'), 'includes --push');
    assert.ok(setUrlArgs.includes('origin'), 'includes remote name');
    assert.ok(setUrlArgs.includes('https://new-url.git'), 'includes url');
  });

  it('renders show, prune, and get_url action args', () => {
    const spec = loader.load('git_remote');

    const showArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'show',
      remote_name: 'origin'
    });
    assert.ok(showArgs.includes('show'), 'includes show');
    assert.ok(showArgs.includes('origin'), 'includes remote name');

    const pruneArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'prune',
      remote_name: 'origin'
    });
    assert.ok(pruneArgs.includes('prune'), 'includes prune');
    assert.ok(pruneArgs.includes('origin'), 'includes remote name');

    const getUrlArgs = engine.renderArgs(spec, {
      path: '.',
      action: 'get_url',
      remote_name: 'origin',
      all: true,
      push_url: true
    });
    assert.ok(getUrlArgs.includes('get-url'), 'includes get-url');
    assert.ok(getUrlArgs.includes('--all'), 'includes --all');
    assert.ok(getUrlArgs.includes('--push'), 'includes --push');
    assert.ok(getUrlArgs.includes('origin'), 'includes remote name');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_remote normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, args = ['remote', '-v']) {
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

  it('parses list output with tab and space separation', () => {
    const stdout = `origin\thttps://github.com/user/repo.git (fetch)
origin  https://github.com/user/repo.git (push)
upstream        https://github.com/upstream/repo.git (fetch)
upstream        https://github.com/upstream/repo.git (push)`;

    const spec = loader.load('git_remote');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_remotes, 2);

    const origin = output.remotes.find(r => r.name === 'origin');
    assert.ok(origin, 'origin exists');
    assert.strictEqual(origin.fetch_url, 'https://github.com/user/repo.git');
    assert.strictEqual(origin.push_url, 'https://github.com/user/repo.git');

    const upstream = output.remotes.find(r => r.name === 'upstream');
    assert.ok(upstream, 'upstream exists');
    assert.strictEqual(upstream.fetch_url, 'https://github.com/upstream/repo.git');
    assert.strictEqual(upstream.push_url, 'https://github.com/upstream/repo.git');
  });

  it('handles empty remote list', () => {
    const stdout = '';
    const spec = loader.load('git_remote');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_remotes, 0);
    assert.deepStrictEqual(output.remotes, []);
  });

  it('parses show output with head_branch, tracked branches, and stale flags', () => {
    const stdout = `* remote origin
  Fetch URL: https://github.com/user/repo.git
  Push  URL: https://github.com/user/repo.git
  HEAD branch: main
  Remote branches:
    main                 tracked
    feature-x            tracked
    old-branch           stale (use 'git remote prune' to remove)
  Local branches configured for 'git pull':
    main                 merges with remote main
    feature-x            merges with remote feature-x
  Local refs configured for 'git push':
    main                 pushes to main (up to date)
    feature-x            pushes to feature-x (local out of date)`;

    const spec = loader.load('git_remote');
    const result = mockResult(stdout, 0, ['remote', 'show', 'origin']);
    const output = normalizer.run(spec, result, { action: 'show', remote_name: 'origin' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_remotes, 1);

    const remote = output.remotes[0];
    assert.strictEqual(remote.name, 'origin');
    assert.strictEqual(remote.fetch_url, 'https://github.com/user/repo.git');
    assert.strictEqual(remote.push_url, 'https://github.com/user/repo.git');
    assert.strictEqual(remote.head_branch, 'main');
    assert.strictEqual(remote.tracked_branches.length, 3);

    const main = remote.tracked_branches.find(b => b.name === 'main');
    assert.ok(main, 'main branch tracked');
    assert.strictEqual(main.tracked, true);
    assert.strictEqual(main.stale, false);

    const old = remote.tracked_branches.find(b => b.name === 'old-branch');
    assert.ok(old, 'old-branch exists');
    assert.strictEqual(old.stale, true);
    assert.strictEqual(old.tracked, false);
  });

  it('sets action-specific output fields', () => {
    const spec = loader.load('git_remote');

    const addOutput = normalizer.run(spec, mockResult('', 0, ['remote', 'add', 'upstream', 'url']), { action: 'add', remote_name: 'upstream' });
    assert.strictEqual(addOutput.added, 'upstream');

    const removeOutput = normalizer.run(spec, mockResult('', 0, ['remote', 'remove', 'upstream']), { action: 'remove', remote_name: 'upstream' });
    assert.strictEqual(removeOutput.removed, 'upstream');

    const renameOutput = normalizer.run(spec, mockResult('', 0, ['remote', 'rename', 'old', 'new']), { action: 'rename', remote_name: 'old', new_name: 'new' });
    assert.strictEqual(renameOutput.renamed, true);
    assert.strictEqual(renameOutput.renamed_from, 'old');
    assert.strictEqual(renameOutput.renamed_to, 'new');

    const setUrlOutput = normalizer.run(spec, mockResult('', 0, ['remote', 'set-url', 'origin', 'url']), { action: 'set_url' });
    assert.strictEqual(setUrlOutput.url_set, true);

    const pruneOutput = normalizer.run(spec, mockResult('', 0, ['remote', 'prune', 'origin']), { action: 'prune' });
    assert.strictEqual(pruneOutput.pruned, true);
  });

  it('produces fallback envelope with used_fallback', () => {
    const spec = loader.load('git_remote');
    // Simulate fallback pipeline
    const fallbackSpec = {
      ...spec,
      normalization: {
        pipeline: [
          { step: 'git_remote_short' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = mockResult('origin\nupstream', 0, ['remote']);
    const output = normalizer.run(fallbackSpec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_remotes, 2);
    assert.strictEqual(output.used_fallback, true);
    assert.strictEqual(output.remotes[0].name, 'origin');
    assert.strictEqual(output.remotes[1].name, 'upstream');
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_remote runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_remote'), 'git_remote is listed');
  });

  it('executes list action on current repo', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_remote', {
      action: 'list',
      path: '.'
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message || JSON.stringify(result)}`);
    assert.ok(Array.isArray(result.remotes), 'remotes is array');
    assert.ok(result.total_remotes >= 0, 'total_remotes is non-negative');

    if (result.total_remotes > 0) {
      const first = result.remotes[0];
      assert.ok(first.name, 'remote has name');
    }
  });
});

console.log('All git_remote tests defined.');
