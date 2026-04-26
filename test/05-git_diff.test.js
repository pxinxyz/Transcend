/**
 * Tests for the git_diff skill — argument rendering, flag handling,
 * validation, structural integrity, and runtime integration.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { NunjucksEngine, TemplateRenderError } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering for all 5 modes ──────────────────────────

describe('git_diff arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders working_tree mode with defaults', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, { mode: 'working_tree', path: '.' });

    assert.ok(args.includes('--color=always'), 'color enabled by default');
    assert.ok(!args.includes('--staged'), 'no staged flag');
    assert.ok(!args.includes('--side-by-side'), 'no side-by-side by default');
  });

  it('renders staged mode', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, { mode: 'staged' });

    assert.ok(args.includes('--staged'), 'staged flag');
  });

  it('renders commit_range mode', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'commit_range',
      source: 'HEAD~1',
      target: 'HEAD'
    });

    assert.ok(args.includes('HEAD~1..HEAD'), 'commit range notation');
  });

  it('renders branch mode', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'branch',
      source: 'main',
      target: 'feature'
    });

    assert.ok(args.includes('main...feature'), 'branch triple-dot notation');
  });

  it('renders file mode', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'file',
      target: 'src/index.js'
    });

    assert.ok(args.includes('--src/index.js'), 'file path prefixed with --');
  });
});

// ── Flag Rendering ─────────────────────────────────────────

describe('git_diff flag rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders side_by_side flag', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'working_tree',
      side_by_side: true
    });

    assert.ok(args.includes('--side-by-side'), 'side-by-side flag');
  });

  it('renders color disabled', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'working_tree',
      color: false
    });

    assert.ok(args.includes('--no-color'), 'no-color flag');
    assert.ok(!args.includes('--color=always'), 'no color-always flag');
  });

  it('renders ignore_whitespace flag', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'working_tree',
      ignore_whitespace: true
    });

    assert.ok(args.includes('-w'), 'ignore-all-space flag');
  });

  it('renders ignore_space_change flag', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'working_tree',
      ignore_space_change: true
    });

    assert.ok(args.includes('-b'), 'ignore-space-change flag');
  });

  it('renders stat_only flag', () => {
    const spec = loader.load('git_diff');
    const args = engine.renderArgs(spec, {
      mode: 'working_tree',
      stat_only: true
    });

    assert.ok(args.includes('--stat'), 'stat flag');
  });
});

// ── Validation ─────────────────────────────────────────────

describe('git_diff validation', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('validates missing required mode', () => {
    const spec = loader.load('git_diff');

    assert.throws(
      () => engine.renderArgs(spec, {}),
      TemplateRenderError,
      'missing mode throws'
    );
    assert.throws(
      () => engine.renderArgs(spec, { path: '.' }),
      TemplateRenderError,
      'missing mode even with path throws'
    );
  });
});

// ── Structural Integrity ───────────────────────────────────

describe('git_diff structural integrity', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });

  it('has required schema fields', () => {
    const spec = loader.load('git_diff');

    assert.strictEqual(spec.name, 'git_diff');
    assert.strictEqual(spec.version, '1.0.0');
    assert.strictEqual(spec.stability, 'stable');
    assert.ok(spec.description, 'has description');

    assert.ok(spec.inputs, 'has inputs');
    assert.ok(spec.inputs.mode.required, 'mode is required');
    assert.ok(spec.inputs.mode.enum, 'mode has enum');

    assert.ok(spec.outputs.success, 'has success schema');
    assert.ok(spec.outputs.error, 'has error schema');

    assert.strictEqual(spec.execution.command, 'delta');
    assert.strictEqual(spec.execution.template_engine, 'nunjucks');
    assert.ok(Array.isArray(spec.execution.args));

    assert.ok(spec.normalization, 'has normalization');
    assert.ok(Array.isArray(spec.normalization.pipeline));
    assert.ok(spec.normalization.pipeline.length > 0);

    assert.ok(spec.resilience.fallback, 'has fallback config');
    assert.strictEqual(spec.resilience.fallback.command, 'git');
    assert.ok(Array.isArray(spec.resilience.fallback.output_gaps));

    assert.ok(spec.chains.compatible_downstream, 'has downstream chains');
    const downstreamNames = spec.chains.compatible_downstream.map(c => c.skill);
    assert.ok(downstreamNames.includes('find_replace'), 'chains to find_replace');
    assert.ok(downstreamNames.includes('file_view'), 'chains to file_view');
    assert.ok(downstreamNames.includes('universal_search'), 'chains to universal_search');
  });
});

// ── Runtime Integration ────────────────────────────────────

describe('git_diff runtime integration', () => {
  it('loads in TranscendRuntime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();

    assert.ok(skills.some(s => s.name === 'git_diff'), 'runtime lists git_diff');

    const result = await rt.execute('git_diff', {});
    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('mode'), 'mentions missing mode');
  });
});

console.log('All git_diff tests defined.');
