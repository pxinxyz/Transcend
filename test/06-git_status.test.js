/**
 * Tests for the git_status skill — arg rendering, schema integrity, and runtime integration.
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

// ── Arg Rendering Tests ────────────────────────────────────

describe('git_status arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders default status with all defaults', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.' });

    assert.ok(args.includes('status'), 'includes status subcommand');
    assert.ok(args.includes('--porcelain'), 'includes porcelain flag');
    assert.ok(args.includes('--branch'), 'includes branch by default');
    assert.ok(args.includes('.'), 'includes path');
    assert.ok(!args.includes('--ignored'), 'no ignored flag by default');
    assert.ok(!args.includes('--untracked-files'), 'no untracked-files override by default');
    assert.ok(!args.includes('--no-renames'), 'no no-renames by default');
  });

  it('renders untracked_files=all', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', untracked_files: 'all' });

    assert.ok(args.includes('--untracked-files=all'), 'includes all untracked flag');
  });

  it('renders untracked_files=normal explicitly', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', untracked_files: 'normal' });

    assert.ok(!args.includes('--untracked-files'), 'omits untracked-files flag for normal mode');
  });

  it('renders untracked_files=no', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', untracked_files: 'no' });

    assert.ok(args.includes('--untracked-files=no'), 'includes no untracked flag');
  });

  it('renders ignored=true', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', ignored: true });

    assert.ok(args.includes('--ignored'), 'includes ignored flag');
  });

  it('renders branch=false', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', branch: false });

    assert.ok(args.includes('--no-branch'), 'includes no-branch flag');
    assert.ok(!args.includes('--branch'), 'omits branch flag');
  });

  it('renders renames=false', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', renames: false });

    assert.ok(args.includes('--no-renames'), 'includes no-renames flag');
  });

  it('renders submodules=true', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.', submodules: true });

    assert.ok(args.includes('--ignore-submodules=none'), 'includes ignore-submodules=none');
  });

  it('renders submodules=false by default', () => {
    const spec = loader.load('git_status');
    const args = engine.renderArgs(spec, { path: '.' });

    assert.ok(args.includes('--ignore-submodules=all'), 'includes ignore-submodules=all by default');
  });
});

// ── Structural Integrity Tests ─────────────────────────────

describe('git_status structural integrity', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });

  it('passes schema validation on load', () => {
    const spec = loader.load('git_status');
    assert.strictEqual(spec.name, 'git_status');
    assert.strictEqual(spec.execution.template_engine, 'nunjucks');
    assert.ok(Array.isArray(spec.execution.args));
    assert.ok(spec.normalization.pipeline.length > 0);
  });

  it('has required output fields', () => {
    const spec = loader.load('git_status');
    const success = spec.outputs.success;

    assert.ok(success, 'success schema exists');
    assert.ok(success.required.includes('status'), 'status is required');
    assert.ok(success.required.includes('is_clean'), 'is_clean is required');
    assert.ok(success.required.includes('files'), 'files is required');
    assert.ok(success.required.includes('summary'), 'summary is required');

    const summary = success.properties.summary;
    assert.ok(summary.required.includes('staged'), 'staged in summary');
    assert.ok(summary.required.includes('modified'), 'modified in summary');
    assert.ok(summary.required.includes('untracked'), 'untracked in summary');
    assert.ok(summary.required.includes('conflicted'), 'conflicted in summary');
    assert.ok(summary.required.includes('ignored'), 'ignored in summary');
  });

  it('has correct chain declarations', () => {
    const spec = loader.load('git_status');
    const chains = spec.chains;

    assert.ok(chains, 'chains section exists');
    assert.ok(Array.isArray(chains.compatible_downstream), 'has downstream array');

    const downstreamSkills = chains.compatible_downstream.map(c => c.skill);
    assert.ok(downstreamSkills.includes('git_diff'), 'chains to git_diff');
    assert.ok(downstreamSkills.includes('find_replace'), 'chains to find_replace');
    assert.ok(downstreamSkills.includes('file_view'), 'chains to file_view');
    assert.ok(downstreamSkills.includes('universal_search'), 'chains to universal_search');
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_status runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('git_status'), 'git_status is listed');
  });

  it('validates invalid enum for untracked_files', () => {
    const engine = new NunjucksEngine();
    const spec = {
      inputs: {
        untracked_files: { type: 'string', enum: ['all', 'normal', 'no'] }
      },
      execution: {
        command: 'git',
        args: ['{{ untracked_files }}'],
        template_engine: 'nunjucks'
      }
    };

    assert.throws(
      () => engine.renderArgs(spec, { untracked_files: 'invalid' }),
      /must be one of/
    );
  });
});

console.log('All git_status tests defined.');
