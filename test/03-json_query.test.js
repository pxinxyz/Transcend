/**
 * Tests for the json_query skill — arg rendering, validation, normalization,
 * truncation, empty output, and runtime integration.
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

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SKILLS_DIR = join(__dirname, '..', 'skills');

// ── Arg Rendering Tests ────────────────────────────────────

describe('json_query arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders basic jq query', () => {
    const spec = loader.load('json_query');
    const args = engine.renderArgs(spec, { query: '.name' });

    assert.ok(args.includes('.name'), 'query included');
  });

  it('renders compact flag', () => {
    const spec = loader.load('json_query');
    const args = engine.renderArgs(spec, { query: '.[]', compact: true });

    assert.ok(args.includes('-c'), 'compact flag included');
  });

  it('renders raw_output flag', () => {
    const spec = loader.load('json_query');
    const args = engine.renderArgs(spec, { query: '.name', raw_output: true });

    assert.ok(args.includes('-r'), 'raw_output flag included');
  });

  it('renders slurp flag', () => {
    const spec = loader.load('json_query');
    const args = engine.renderArgs(spec, { query: '.[]', slurp: true });

    assert.ok(args.includes('-s'), 'slurp flag included');
  });

  it('validates missing required query', () => {
    const spec = loader.load('json_query');
    assert.throws(
      () => engine.renderArgs(spec, {}),
      TemplateRenderError,
      'missing query should throw'
    );
  });

});

// ── Normalization Tests ────────────────────────────────────

describe('json_query normalization', () => {
  it('parses jq NDJSON output', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'json_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_jq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '{"name":"alice"}\n{"name":"bob"}\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 2);
    assert.deepStrictEqual(output.results, [{ name: 'alice' }, { name: 'bob' }]);
    assert.strictEqual(output.truncated, false);
  });

  it('parses pretty-printed jq output', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'json_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_jq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '{\n  "name": "alice"\n}\n{\n  "name": "bob"\n}\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 2);
    assert.deepStrictEqual(output.results, [{ name: 'alice' }, { name: 'bob' }]);
  });

  it('truncates results over max_results', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'json_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_jq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const stdout = Array.from({ length: 5 }, (_, i) => JSON.stringify({ id: i })).join('\n') + '\n';
    const result = new ExecutionResult({
      exitCode: 0,
      stdout,
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { max_results: 3 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 3);
    assert.strictEqual(output.truncated, true);
  });

  it('handles empty output', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'json_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_jq_output' },
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
    assert.strictEqual(output.total_results, 0);
    assert.deepStrictEqual(output.results, []);
    assert.strictEqual(output.truncated, false);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('json_query runtime integration', () => {
  it('loads json_query spec', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const spec = loader.load('json_query');

    assert.strictEqual(spec.name, 'json_query');
    assert.strictEqual(spec.execution.command, 'jq');
    assert.ok(Array.isArray(spec.execution.args));
    assert.ok(spec.normalization.pipeline.length > 0);
  });

  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('json_query'), 'json_query is listed');
  });

  it('executes jq successfully when available', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('json_query', {
      query: '.name',
      input: '{"name":"test"}',
      compact: true
    });

    assert.strictEqual(result.status, 'ok');
    assert.ok(Array.isArray(result.results));
    assert.strictEqual(result.total_results, 1);
    assert.strictEqual(result.results[0], 'test');
  });

  it('validates missing query at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('json_query', {});

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('query'), 'mentions missing query');
  });
});

console.log('All json_query tests defined.');
