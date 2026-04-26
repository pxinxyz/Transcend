/**
 * Integration tests for the full TranscendRuntime.
 * Tests end-to-end skill loading, rendering, and execution.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { NunjucksEngine } from '../lib/NunjucksEngine.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

describe('TranscendRuntime', () => {
  it('creates runtime with default options', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    assert.ok(rt instanceof TranscendRuntime);
  });

  it('lists available skills', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();

    assert.ok(skills.length >= 3, 'should find at least 3 skills');

    const names = skills.map(s => s.name).sort();
    assert.ok(names.includes('codebase_analysis'), 'has codebase_analysis');
    assert.ok(names.includes('find_replace'), 'has find_replace');
    assert.ok(names.includes('universal_search'), 'has universal_search');
  });

  it('returns error for unknown skill', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('nonexistent_skill', {});

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'skill_not_found');
  });

  it('validates missing required inputs', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('universal_search', {});

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('pattern'), 'mentions missing pattern');
  });

  it('validates input types', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('universal_search', {
      pattern: 123,  // should be string
      path: '.'
    });

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
  });
});

describe('SkillLoader', () => {
  it('loads universal_search spec', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const spec = loader.load('universal_search');

    assert.strictEqual(spec.name, 'universal_search');
    assert.strictEqual(spec.execution.template_engine, 'nunjucks');
    assert.ok(Array.isArray(spec.execution.args));
    assert.ok(spec.normalization.pipeline.length > 0);
  });

  it('loads codebase_analysis spec', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const spec = loader.load('codebase_analysis');

    assert.strictEqual(spec.name, 'codebase_analysis');
    assert.strictEqual(spec.execution.command, 'scc');
    assert.ok(spec.resilience?.fallback, 'has fallback config');
  });

  it('caches loaded specs', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const spec1 = loader.load('universal_search');
    const spec2 = loader.load('universal_search');
    assert.strictEqual(spec1, spec2);
  });
});

describe('NunjucksEngine', () => {
  it('renders universal_search templates', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const engine = new NunjucksEngine();
    const spec = loader.load('universal_search');

    const args = engine.renderArgs(spec, {
      pattern: 'TODO',
      path: './src',
      ignore_case: true,
      file_types: ['ts']
    });

    assert.ok(args.includes('--json'));
    assert.ok(args.includes('--ignore-case'));
    assert.ok(args.includes('--type=ts'));
    assert.ok(args.includes('TODO'));
    assert.ok(args.includes('./src'));
  });

  it('validates enum inputs', () => {
    const engine = new NunjucksEngine();
    const spec = {
      inputs: {
        level: { type: 'string', enum: ['low', 'medium', 'high'] }
      },
      execution: {
        command: 'test',
        args: ['{{ level }}'],
        template_engine: 'nunjucks'
      }
    };

    assert.throws(
      () => engine.renderArgs(spec, { level: 'invalid' }),
      /must be one of/
    );
  });

  it('validates integer ranges', () => {
    const engine = new NunjucksEngine();
    const spec = {
      inputs: {
        count: { type: 'integer', minimum: 0, maximum: 100 }
      },
      execution: {
        command: 'test',
        args: ['{{ count }}'],
        template_engine: 'nunjucks'
      }
    };

    assert.throws(() => engine.renderArgs(spec, { count: -1 }), /must be >= 0/);
    assert.throws(() => engine.renderArgs(spec, { count: 101 }), /must be <= 100/);
    // Boundary values should work
    const args1 = engine.renderArgs(spec, { count: 0 });
    assert.ok(args1);
    const args2 = engine.renderArgs(spec, { count: 100 });
    assert.ok(args2);
  });
});

console.log('All runtime integration tests defined.');
