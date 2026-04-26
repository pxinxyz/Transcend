/**
 * Tests for the git_tag skill — arg rendering, normalization pipeline,
 * truncation, empty results, action-specific fields, fallback envelope,
 * and runtime integration.
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

describe('git_tag arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders list action with defaults', () => {
    const spec = loader.load('git_tag');
    const args = engine.renderArgs(spec, { path: '.', action: 'list' });

    assert.ok(args.includes('tag'), 'includes tag subcommand');
    assert.ok(args.includes('-n1'), 'includes -n1');
    assert.ok(args.includes('.'), 'includes path');
    assert.ok(!args.includes('--sort'), 'no sort by default');
  });

  it('renders create action with options', () => {
    const spec = loader.load('git_tag');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'create',
      tag_name: 'v1.0.0',
      message: 'First release',
      annotate: true,
      object: 'abc1234',
      force: true
    });

    assert.ok(args.includes('tag'), 'includes tag subcommand');
    assert.ok(args.includes('-a'), 'includes -a for annotated');
    assert.ok(args.includes('-m'), 'includes -m for message');
    assert.ok(args.includes('First release'), 'includes message');
    assert.ok(args.includes('-f'), 'includes -f for force');
    assert.ok(args.includes('v1.0.0'), 'includes tag name');
    assert.ok(args.includes('abc1234'), 'includes object');
  });

  it('renders delete action args', () => {
    const spec = loader.load('git_tag');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'delete',
      tag_name: 'v1.0.0'
    });

    assert.ok(args.includes('tag'), 'includes tag subcommand');
    assert.ok(args.includes('-d'), 'includes -d for delete');
    assert.ok(args.includes('v1.0.0'), 'includes tag name');
  });

  it('renders show action args', () => {
    const spec = loader.load('git_tag');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'show',
      tag_name: 'v1.0.0'
    });

    assert.ok(args.includes('show'), 'includes show subcommand');
    assert.ok(args.includes('--quiet'), 'includes --quiet');
    assert.ok(args.includes('v1.0.0'), 'includes tag name');
    assert.ok(!args.includes('tag'), 'no tag subcommand for show');
  });

  it('renders verify action args', () => {
    const spec = loader.load('git_tag');
    const args = engine.renderArgs(spec, {
      path: '.',
      action: 'verify',
      tag_name: 'v1.0.0'
    });

    assert.ok(args.includes('tag'), 'includes tag subcommand');
    assert.ok(args.includes('-l'), 'includes -l for list/verify');
    assert.ok(args.includes('v1.0.0'), 'includes tag name');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_tag normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, args = ['tag', 'list']) {
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

  it('parses annotated tag from git show --quiet output', () => {
    const stdout = `tag v1.0.0
Tagger: Alice <alice@example.com>
Date:   Mon Jan 1 12:00:00 2024 +0000

First release

commit abc123def456abc123def456abc123def456abc1
Author: Bob <bob@example.com>
Date:   Sun Dec 31 12:00:00 2023 +0000

    Initial commit
`;

    const spec = loader.load('git_tag');
    const result = mockResult(stdout, 0, ['show', '--quiet', 'v1.0.0']);
    const output = normalizer.run(spec, result, { action: 'show', tag_name: 'v1.0.0' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 1);
    const tag = output.tags[0];
    assert.strictEqual(tag.name, 'v1.0.0');
    assert.strictEqual(tag.annotated, true);
    assert.strictEqual(tag.tagger, 'Alice <alice@example.com>');
    assert.ok(tag.date);
    assert.strictEqual(tag.message, 'First release');
    assert.strictEqual(tag.annotation, 'First release');
    assert.strictEqual(tag.object, 'abc123def456abc123def456abc123def456abc1');
  });

  it('detects lightweight tag from git show --quiet output', () => {
    const stdout = `commit abc123def456abc123def456abc123def456abc1 (tag: v0.9.0)
Author: Bob <bob@example.com>
Date:   Sun Dec 31 12:00:00 2023 +0000

    Initial commit
`;

    const spec = loader.load('git_tag');
    const result = mockResult(stdout, 0, ['show', '--quiet', 'v0.9.0']);
    const output = normalizer.run(spec, result, { action: 'show', tag_name: 'v0.9.0' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 1);
    const tag = output.tags[0];
    assert.strictEqual(tag.name, 'v0.9.0');
    assert.strictEqual(tag.annotated, false);
    assert.strictEqual(tag.object, 'abc123def456abc123def456abc123def456abc1');
    assert.strictEqual(tag.tagger, undefined);
    assert.strictEqual(tag.message, undefined);
  });

  it('parses git tag -n1 list output', () => {
    const stdout = `v1.0.0          First release
v1.1.0          Second release
v2.0.0
`;

    const spec = loader.load('git_tag');
    const result = mockResult(stdout, 0, ['tag', '-n1']);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 3);

    const t1 = output.tags.find(t => t.name === 'v1.0.0');
    assert.ok(t1);
    assert.strictEqual(t1.annotation, 'First release');
    assert.strictEqual(t1.annotated, true);

    const t2 = output.tags.find(t => t.name === 'v1.1.0');
    assert.ok(t2);
    assert.strictEqual(t2.annotation, 'Second release');
    assert.strictEqual(t2.annotated, true);

    const t3 = output.tags.find(t => t.name === 'v2.0.0');
    assert.ok(t3);
    assert.strictEqual(t3.annotation, '');
    assert.strictEqual(t3.annotated, false);
  });

  it('truncates tags exceeding max_results', () => {
    const stdout = `v1.0.0
v1.1.0
v1.2.0
v1.3.0
v1.4.0
`;

    const spec = loader.load('git_tag');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list', max_results: 2 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 2);
    assert.strictEqual(output.truncated, true);
    assert.strictEqual(output.tags.length, 2);
  });

  it('handles empty tag list', () => {
    const stdout = '';
    const spec = loader.load('git_tag');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 0);
    assert.deepStrictEqual(output.tags, []);
    assert.strictEqual(output.truncated, false);
  });

  it('includes action-specific output fields', () => {
    const spec = loader.load('git_tag');

    const createOutput = normalizer.run(spec, mockResult('', 0, ['tag', 'v1.0.0']), { action: 'create', tag_name: 'v1.0.0' });
    assert.strictEqual(createOutput.created, 'v1.0.0');
    assert.strictEqual(createOutput.deleted, undefined);
    assert.strictEqual(createOutput.verified, undefined);

    const deleteOutput = normalizer.run(spec, mockResult("Deleted tag 'v1.0.0'", 0, ['tag', '-d', 'v1.0.0']), { action: 'delete', tag_name: 'v1.0.0' });
    assert.strictEqual(deleteOutput.deleted, 'v1.0.0');
    assert.strictEqual(deleteOutput.created, undefined);

    const verifyOutput = normalizer.run(spec, mockResult('v1.0.0', 0, ['tag', '-l', 'v1.0.0']), { action: 'verify', tag_name: 'v1.0.0' });
    assert.strictEqual(verifyOutput.verified, true);
    assert.strictEqual(verifyOutput.tags.length, 1);

    const verifyMissingOutput = normalizer.run(spec, mockResult('', 0, ['tag', '-l', 'missing']), { action: 'verify', tag_name: 'missing' });
    assert.strictEqual(verifyMissingOutput.verified, false);
    assert.strictEqual(verifyMissingOutput.tags.length, 0);
  });

  it('produces fallback envelope via git_tag_short', () => {
    const spec = {
      name: 'git_tag',
      inputs: { max_results: { default: 100 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'git_tag_short' },
          { step: 'truncate_tags' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = mockResult('v1.0.0\nv1.1.0\nv2.0.0');
    const output = normalizer.run(spec, result, { action: 'list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_tags, 3);
    assert.strictEqual(output.tags[0].name, 'v1.0.0');
    assert.strictEqual(output.tags[0].annotation, '');
    assert.strictEqual(output.tags[0].annotated, false);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_tag runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_tag'), 'git_tag is listed');
  });

  it('executes git_tag list on this repo', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_tag', {
      action: 'list',
      path: '.'
    });

    assert.strictEqual(result.status, 'ok');
    assert.ok(Array.isArray(result.tags), 'tags is array');
    assert.ok(typeof result.total_tags === 'number', 'total_tags is number');
  });
});

console.log('All git_tag tests defined.');
