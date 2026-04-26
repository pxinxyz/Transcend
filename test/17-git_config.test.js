/**
 * Tests for the git_config skill — arg rendering, normalization pipeline
 * parsing, empty results, action-specific fields, and runtime integration.
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

describe('git_config arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders list action with show-origin and show-scope', () => {
    const spec = loader.load('git_config');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'list',
      show_origin: true,
      show_scope: true
    });

    assert.ok(args.includes('config'), 'includes config subcommand');
    assert.ok(args.includes('--list'), 'includes --list');
    assert.ok(args.includes('--show-origin'), 'includes --show-origin');
    assert.ok(args.includes('--show-scope'), 'includes --show-scope');
    assert.ok(args.includes('.'), 'includes path');
  });

  it('renders get action args', () => {
    const spec = loader.load('git_config');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'get',
      key: 'user.name'
    });

    assert.ok(args.includes('config'), 'includes config subcommand');
    assert.ok(args.includes('user.name'), 'includes key');
    assert.ok(!args.includes('--list'), 'no --list for get');
    assert.ok(!args.includes('--get-all'), 'no --get-all for get');
  });

  it('renders get_all action args', () => {
    const spec = loader.load('git_config');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'get_all',
      key: 'core.editor'
    });

    assert.ok(args.includes('config'), 'includes config subcommand');
    assert.ok(args.includes('--get-all'), 'includes --get-all');
    assert.ok(args.includes('core.editor'), 'includes key');
  });

  it('renders set action args', () => {
    const spec = loader.load('git_config');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'set',
      key: 'user.email',
      value: 'test@example.com'
    });

    assert.ok(args.includes('config'), 'includes config subcommand');
    assert.ok(args.includes('user.email'), 'includes key');
    assert.ok(args.includes('test@example.com'), 'includes value');
    assert.ok(!args.includes('--unset'), 'no --unset for set');
  });

  it('renders unset action args', () => {
    const spec = loader.load('git_config');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'unset',
      key: 'user.name'
    });

    assert.ok(args.includes('config'), 'includes config subcommand');
    assert.ok(args.includes('--unset'), 'includes --unset');
    assert.ok(args.includes('user.name'), 'includes key');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_config normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, args = ['config', '--list']) {
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

  it('parses list output with show-origin and show-scope', () => {
    const stdout = `local:.git/config\tcore.repositoryformatversion\t0
local:.git/config\tcore.filemode\ttrue
global:/home/user/.gitconfig\tuser.name\tTest User
system:/etc/gitconfig\tcore.editor\tvim`;

    const spec = loader.load('git_config');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'list');
    assert.strictEqual(output.total_entries, 4);
    assert.strictEqual(output.entries.length, 4);

    const first = output.entries[0];
    assert.strictEqual(first.key, 'core.repositoryformatversion');
    assert.strictEqual(first.value, '0');
    assert.strictEqual(first.scope, 'local');
    assert.strictEqual(first.origin, '.git/config');

    const globalEntry = output.entries.find(e => e.key === 'user.name');
    assert.ok(globalEntry, 'global entry exists');
    assert.strictEqual(globalEntry.scope, 'global');
    assert.strictEqual(globalEntry.origin, '/home/user/.gitconfig');
  });

  it('parses get output (single value)', () => {
    const stdout = 'Test User';
    const spec = loader.load('git_config');
    const result = mockResult(stdout, 0, ['config', 'user.name']);
    const output = normalizer.run(spec, result, { action: 'get', key: 'user.name' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'get');
    assert.strictEqual(output.total_entries, 1);
    assert.strictEqual(output.entries[0].key, 'user.name');
    assert.strictEqual(output.entries[0].value, 'Test User');
    assert.strictEqual(output.value, 'Test User');
  });

  it('parses get_all output (multi-value)', () => {
    const stdout = `vim
nano
emacs`;
    const spec = loader.load('git_config');
    const result = mockResult(stdout, 0, ['config', '--get-all', 'core.editor']);
    const output = normalizer.run(spec, result, { action: 'get_all', key: 'core.editor' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'get_all');
    assert.strictEqual(output.total_entries, 3);
    assert.deepStrictEqual(output.values, ['vim', 'nano', 'emacs']);
    assert.strictEqual(output.entries[0].key, 'core.editor');
    assert.strictEqual(output.entries[1].key, 'core.editor');
    assert.strictEqual(output.entries[2].key, 'core.editor');
  });

  it('parses set confirmation', () => {
    const spec = loader.load('git_config');
    const result = mockResult('', 0, ['config', 'user.email', 'test@example.com']);
    const output = normalizer.run(spec, result, {
      action: 'set',
      key: 'user.email',
      value: 'test@example.com'
    });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'set');
    assert.strictEqual(output.set, true);
    assert.strictEqual(output.key, 'user.email');
    assert.strictEqual(output.value, 'test@example.com');
    assert.deepStrictEqual(output.entries, []);
  });

  it('parses unset confirmation', () => {
    const spec = loader.load('git_config');
    const result = mockResult('', 0, ['config', '--unset', 'user.name']);
    const output = normalizer.run(spec, result, { action: 'unset', key: 'user.name' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'unset');
    assert.strictEqual(output.unset, true);
    assert.strictEqual(output.key, 'user.name');
    assert.deepStrictEqual(output.entries, []);
  });

  it('handles empty config output', () => {
    const stdout = '';
    const spec = loader.load('git_config');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'list');
    assert.strictEqual(output.total_entries, 0);
    assert.deepStrictEqual(output.entries, []);
  });

  it('produces fallback envelope for unknown pipeline', () => {
    const spec = { ...loader.load('git_config') };
    spec.normalization = { input_format: 'plaintext', pipeline: [] };
    const result = mockResult('some raw output');
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.raw_output, 'some raw output');
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_config runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_config'), 'git_config is listed');
  });

  it('integrates with runtime for list action', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_config', {
      path: '.',
      action: 'list'
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message || JSON.stringify(result)}`);
    assert.ok(Array.isArray(result.entries), 'entries is array');
    assert.strictEqual(typeof result.total_entries, 'number');
    assert.ok(result.total_entries >= 0, 'total_entries is non-negative');
  });
});

console.log('All git_config tests defined.');
