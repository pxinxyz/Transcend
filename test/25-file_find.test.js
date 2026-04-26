/**
 * Tests for file_find skill — fd/find filesystem discovery.
 * Covers arg rendering, validation, normalization pipeline, truncation,
 * fallback envelope, and runtime integration.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { NunjucksEngine, TemplateRenderError } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';
import { NormalizationPipeline } from '../lib/NormalizationPipeline.js';
import { ExecutionResult } from '../lib/CommandExecutor.js';
import { TranscendRuntime, createRuntime } from '../lib/TranscendRuntime.js';
import { ResilienceHandler } from '../lib/ResilienceHandler.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ═══════════════════════════════════════════════════════════════
// Arg Rendering Tests (5)
// ═══════════════════════════════════════════════════════════════

describe('file_find arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders fd with pattern and default path', () => {
    const spec = loader.load('file_find');
    const args = engine.renderArgs(spec, { pattern: 'test', path: '.' });

    assert.ok(args.includes('test'), 'pattern included');
    assert.ok(args.includes('.'), 'path included');
  });

  it('renders type filter as fd shorthand', () => {
    const spec = loader.load('file_find');
    const argsFile = engine.renderArgs(spec, { pattern: 'foo', type: 'file' });
    assert.ok(argsFile.includes('-t'), 'type flag for file');
    assert.ok(argsFile.includes('f'), 'type value f');

    const argsDir = engine.renderArgs(spec, { pattern: 'foo', type: 'directory' });
    assert.ok(argsDir.includes('-t'), 'type flag for directory');
    assert.ok(argsDir.includes('d'), 'type value d');

    const argsAny = engine.renderArgs(spec, { pattern: 'foo', type: 'any' });
    assert.ok(!argsAny.includes('-t'), 'no type flag for any');
  });

  it('renders extension and max_depth', () => {
    const spec = loader.load('file_find');
    const args = engine.renderArgs(spec, { pattern: 'foo', extension: 'ts', max_depth: 3 });

    assert.ok(args.includes('-e'), 'extension flag');
    assert.ok(args.includes('ts'), 'extension value');
    assert.ok(args.includes('-d'), 'max_depth flag');
    assert.ok(args.includes('3'), 'max_depth value');
  });

  it('renders hidden and no_ignore flags', () => {
    const spec = loader.load('file_find');
    const args = engine.renderArgs(spec, { pattern: 'foo', hidden: true, no_ignore: true });

    assert.ok(args.includes('-H'), 'hidden flag');
    assert.ok(args.includes('-I'), 'no_ignore flag');
  });

  it('renders size and changed_within filters', () => {
    const spec = loader.load('file_find');
    const args = engine.renderArgs(spec, { pattern: 'foo', size: '+1k', changed_within: '1h' });

    assert.ok(args.some(a => a.includes('--size')), 'size flag');
    assert.ok(args.includes('+1k'), 'size value');
    assert.ok(args.some(a => a.includes('--changed-within')), 'changed_within flag');
    assert.ok(args.includes('1h'), 'changed_within value');
  });
});

// ═══════════════════════════════════════════════════════════════
// Validation Tests (1)
// ═══════════════════════════════════════════════════════════════

describe('file_find validation', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('validates missing required pattern', () => {
    const spec = loader.load('file_find');
    assert.throws(() => engine.renderArgs(spec, { path: '.' }), TemplateRenderError);
  });
});

// ═══════════════════════════════════════════════════════════════
// Normalization Tests (2)
// ═══════════════════════════════════════════════════════════════

describe('file_find normalization', () => {
  it('parse_fd_output produces file objects from paths', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'file_find',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'parse_fd_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'src/main.ts\nsrc/lib.rs\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { type: 'file' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_files, 2);
    assert.strictEqual(output.files[0].path, 'src/main.ts');
    assert.strictEqual(output.files[0].name, 'main.ts');
    assert.strictEqual(output.files[0].type, 'file');
    assert.strictEqual(output.files[1].path, 'src/lib.rs');
    assert.strictEqual(output.files[1].name, 'lib.rs');
    assert.strictEqual(output.truncated, false);
  });

  it('parse_fd_output handles empty stdout', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'file_find',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'parse_fd_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_files, 0);
    assert.deepStrictEqual(output.files, []);
    assert.strictEqual(output.truncated, false);
  });
});

// ═══════════════════════════════════════════════════════════════
// Truncation Test (1)
// ═══════════════════════════════════════════════════════════════

describe('file_find truncation', () => {
  it('truncate_results caps files at max_results', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'file_find',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'parse_fd_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'a.txt\nb.txt\nc.txt\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { max_results: 2 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_files, 2);
    assert.strictEqual(output.files.length, 2);
    assert.strictEqual(output.truncated, true);
  });
});

// ═══════════════════════════════════════════════════════════════
// Fallback Envelope Test (1)
// ═══════════════════════════════════════════════════════════════

describe('file_find fallback envelope', () => {
  it('includes used_fallback and output gap warnings', () => {
    const handler = new ResilienceHandler();
    const spec = {
      resilience: {
        fallback: {
          command: 'find',
          output_gaps: [
            'no size or modified timestamps',
            'no .gitignore awareness',
            'pattern treated as glob substring instead of regex'
          ]
        }
      }
    };
    const envelope = {
      status: 'ok',
      files: [{ path: 'a.txt', name: 'a.txt', type: 'file' }],
      total_files: 1,
      truncated: false,
      warnings: []
    };
    const result = handler.annotateFallback(envelope, spec);

    assert.strictEqual(result.used_fallback, true);
    assert.ok(result.warnings.some(w => w.includes('Fallback mode')), 'fallback warning');
    assert.ok(result.warnings.some(w => w.includes('no size')), 'size gap warning');
    assert.ok(result.warnings.some(w => w.includes('.gitignore')), 'gitignore gap warning');
  });
});

// ═══════════════════════════════════════════════════════════════
// Runtime Integration Tests (2)
// ═══════════════════════════════════════════════════════════════

describe('file_find runtime integration', () => {
  it('loads and lists file_find skill', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const spec = loader.load('file_find');
    assert.strictEqual(spec.name, 'file_find');
    assert.ok(spec.execution, 'has execution config');
    assert.ok(spec.normalization, 'has normalization config');
    assert.ok(spec.resilience?.fallback, 'has fallback config');

    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);
    assert.ok(names.includes('file_find'), 'file_find is listed');
  });

  it('executes file_find or returns proper envelope', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('file_find', { pattern: 'test', path: '.' });

    assert.ok(
      result.status === 'ok' || result.status === 'error',
      'returns a valid envelope'
    );
    if (result.status === 'error') {
      assert.ok(
        ['command_not_found', 'invalid_argument', 'timeout', 'unknown'].includes(result.error_type),
        'error type is valid'
      );
    }
  });
});

console.log('All file_find tests defined.');
