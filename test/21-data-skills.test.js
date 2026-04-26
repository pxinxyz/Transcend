/**
 * Tests for the data processing skills — yaml_query, column_extract, csv_analysis.
 * Covers arg rendering, validation, normalization pipeline, and runtime integration.
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

describe('yaml_query arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders query with input_format and output_format flags', () => {
    const spec = loader.load('yaml_query');
    const args = engine.renderArgs(spec, { query: '.name', input_format: 'yaml', output_format: 'json' });

    assert.ok(args.includes('.name'), 'query included');
    assert.ok(args.some(a => a.includes('-pyaml')), 'input_format flag included');
    assert.ok(args.some(a => a.includes('-ojson')), 'output_format flag included');
  });
});

describe('column_extract arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders columns and delimiter flags', () => {
    const spec = loader.load('column_extract');
    const args = engine.renderArgs(spec, { columns: [1, 3], delimiter: ',' });

    assert.ok(args.includes('0'), 'first column converted to 0-based');
    assert.ok(args.includes('2'), 'second column converted to 0-based');
    assert.ok(args.some(a => a === '-f' || a.startsWith('-f')), 'delimiter flag included');
  });
});

describe('csv_analysis arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders operation and delimiter flags', () => {
    const spec = loader.load('csv_analysis');
    const args = engine.renderArgs(spec, { operation: 'count', delimiter: ',' });

    assert.ok(args.includes('count'), 'operation included');
    assert.ok(args.includes('--json'), 'json flag included for count');
    assert.ok(args.some(a => a === '-d' || a.startsWith('-d')), 'delimiter flag included');
  });
});

// ── Validation Tests ───────────────────────────────────────

describe('data skills validation', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('validates missing required query for yaml_query', () => {
    const spec = loader.load('yaml_query');
    assert.throws(
      () => engine.renderArgs(spec, {}),
      TemplateRenderError,
      'missing query should throw'
    );
  });

  it('validates invalid operation for csv_analysis', () => {
    const spec = loader.load('csv_analysis');
    assert.throws(
      () => engine.renderArgs(spec, { operation: 'invalid_op' }),
      TemplateRenderError,
      'invalid operation should throw'
    );
  });
});

// ── Normalization / Structural Integrity Tests ─────────────

describe('yaml_query normalization', () => {
  it('parse_yq_output handles scalar JSON', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'yaml_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_yq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '"hello"\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 1);
    assert.deepStrictEqual(output.results, ['hello']);
  });

  it('parse_yq_output handles object JSON', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'yaml_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_yq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '{"name":"alice","age":30}\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 1);
    assert.deepStrictEqual(output.results, [{ name: 'alice', age: 30 }]);
  });

  it('parse_yq_output handles array JSON', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'yaml_query',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'ndjson',
        pipeline: [
          { step: 'parse_yq_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '[1, 2, 3]\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 1);
    assert.deepStrictEqual(output.results, [[1, 2, 3]]);
  });
});

describe('column_extract normalization', () => {
  it('parse_choose_output handles tabular output', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'column_extract',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'parse_choose_output' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'alice\t30\nbob\t25\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { delimiter: '\t', output_delimiter: '\t' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 2);
    assert.deepStrictEqual(output.results, [
      { columns: ['alice', '30'] },
      { columns: ['bob', '25'] }
    ]);
  });
});

describe('csv_analysis normalization', () => {
  it('parse_json populates results and truncates', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'csv_analysis',
      inputs: { max_results: { default: 1000 } },
      normalization: {
        input_format: 'json',
        pipeline: [
          { step: 'parse_json' },
          { step: 'truncate_results' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify([{ name: 'alice' }, { name: 'bob' }, { name: 'charlie' }]),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { limit: 2 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_results, 2);
    assert.strictEqual(output.truncated, true);
    assert.deepStrictEqual(output.results, [{ name: 'alice' }, { name: 'bob' }]);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('data skills runtime integration', () => {
  it('loads all three data skills', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const yamlSpec = loader.load('yaml_query');
    const colSpec = loader.load('column_extract');
    const csvSpec = loader.load('csv_analysis');

    assert.strictEqual(yamlSpec.name, 'yaml_query');
    assert.strictEqual(colSpec.name, 'column_extract');
    assert.strictEqual(csvSpec.name, 'csv_analysis');
  });

  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('yaml_query'), 'yaml_query is listed');
    assert.ok(names.includes('column_extract'), 'column_extract is listed');
    assert.ok(names.includes('csv_analysis'), 'csv_analysis is listed');
  });

  it('executes csv_analysis count with qsv', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('csv_analysis', {
      operation: 'count',
      input: 'name,age\nalice,30\nbob,25\n',
      delimiter: ','
    });

    assert.strictEqual(result.status, 'ok');
    assert.ok(Array.isArray(result.results));
    assert.strictEqual(result.total_results, 1);
    assert.strictEqual(result.results[0].count, 2);
  });

  it('validates yaml_query missing query at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('yaml_query', {});

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('query'), 'mentions missing query');
  });
});

console.log('All data skills tests defined.');
