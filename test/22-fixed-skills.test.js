/**
 * Tests for the 10 fixed skill stubs — benchmark_execution, file_view, github_api,
 * js_lint_fix, process_list, python_lint_fix, rust_test, semantic_diff,
 * structural_search, directory_jump.
 *
 * Covers: arg rendering, validation, normalization pipeline, structural integrity,
 * and runtime integration.
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

// ═══════════════════════════════════════════════════════════════
// Arg Rendering Tests (10)
// ═══════════════════════════════════════════════════════════════

describe('benchmark_execution arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders hyperfine with command and defaults', () => {
    const spec = loader.load('benchmark_execution');
    const args = engine.renderArgs(spec, { command: 'echo hello' });

    assert.ok(args.includes('echo hello'), 'command included');
    assert.ok(args.some(a => a.includes('--warmup')), 'warmup flag included');
    assert.ok(args.some(a => a.includes('--runs')), 'runs flag included');
    assert.ok(args.some(a => a.includes('--export-json')), 'export-json flag included');
  });

  it('renders parameter scan flags', () => {
    const spec = loader.load('benchmark_execution');
    const args = engine.renderArgs(spec, {
      command: 'echo {{i}}',
      parameter_scan: { name: 'i', min: 1, max: 5 }
    });

    assert.ok(args.some(a => a === '-P'), 'parameter scan flag');
    assert.ok(args.includes('i'), 'parameter name included');
    assert.ok(args.includes('1'), 'parameter min included');
    assert.ok(args.includes('5'), 'parameter max included');
  });
});

describe('file_view arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders bat with line range and theme', () => {
    const spec = loader.load('file_view');
    const args = engine.renderArgs(spec, {
      file: 'src/main.rs',
      line_range: '10:20',
      theme: 'TwoDark',
      plain: false
    });

    assert.ok(args.includes('src/main.rs'), 'file included');
    assert.ok(args.some(a => a.includes('--line-range')), 'line-range flag');
    assert.ok(args.some(a => a.includes('10:20')), 'line range value');
    assert.ok(args.some(a => a.includes('--theme=TwoDark')), 'theme included');
  });

  it('renders plain flag when enabled', () => {
    const spec = loader.load('file_view');
    const args = engine.renderArgs(spec, { file: 'test.txt', plain: true });

    assert.ok(args.some(a => a === '--plain'), 'plain flag included');
  });
});

describe('github_api arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders pr_list with json flag', () => {
    const spec = loader.load('github_api');
    const args = engine.renderArgs(spec, { action: 'pr_list' });

    assert.ok(args.includes('pr'), 'pr subcommand');
    assert.ok(args.includes('list'), 'list subcommand');
    assert.ok(args.some(a => a.includes('--json')), 'json flag included');
  });

  it('renders repo_view with repo arg', () => {
    const spec = loader.load('github_api');
    const args = engine.renderArgs(spec, { action: 'repo_view', args: { repo: 'owner/repo' } });

    assert.ok(args.includes('repo'), 'repo subcommand');
    assert.ok(args.includes('view'), 'view subcommand');
  });
});

describe('js_lint_fix arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders biome lint with write flag', () => {
    const spec = loader.load('js_lint_fix');
    const args = engine.renderArgs(spec, { path: 'src', action: 'lint', write: true });

    assert.ok(args.includes('lint'), 'lint action');
    assert.ok(args.includes('--write'), 'write flag');
    assert.ok(args.includes('--json'), 'json flag');
    assert.ok(args.includes('src'), 'path included');
  });
});

describe('process_list arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders procs with tree and json flags', () => {
    const spec = loader.load('process_list');
    const args = engine.renderArgs(spec, { tree: true, keyword: 'node' });

    assert.ok(args.includes('node'), 'keyword included');
    assert.ok(args.some(a => a === '--tree'), 'tree flag');
    assert.ok(args.some(a => a === '--json'), 'json flag');
  });
});

describe('python_lint_fix arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders ruff check with select and ignore', () => {
    const spec = loader.load('python_lint_fix');
    const args = engine.renderArgs(spec, {
      path: 'src',
      action: 'check',
      select: 'E,W',
      ignore: 'E501'
    });

    assert.ok(args.includes('check'), 'check action');
    assert.ok(args.some(a => a.includes('--select')), 'select flag');
    assert.ok(args.includes('E,W'), 'select value');
    assert.ok(args.some(a => a.includes('--ignore')), 'ignore flag');
  });
});

describe('rust_test arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders cargo nextest run with profile and filter', () => {
    const spec = loader.load('rust_test');
    const args = engine.renderArgs(spec, {
      action: 'run',
      profile: 'ci',
      filter: 'test_foo',
      json_output: true
    });

    assert.ok(args.includes('nextest'), 'nextest included');
    assert.ok(args.includes('run'), 'run action');
    assert.ok(args.some(a => a.includes('--profile')), 'profile flag');
    assert.ok(args.includes('ci'), 'profile value');
    assert.ok(args.some(a => a.includes('--message-format')), 'message-format flag');
  });
});

describe('semantic_diff arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders delta with side-by-side and theme', () => {
    const spec = loader.load('semantic_diff');
    const args = engine.renderArgs(spec, {
      diff_input: 'diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n+new',
      side_by_side: true,
      theme: 'GitHub'
    });

    assert.ok(args.some(a => a === '--side-by-side'), 'side-by-side flag');
    assert.ok(args.some(a => a.includes('--theme=GitHub')), 'theme included');
    assert.ok(args.some(a => a === '--no-gitconfig'), 'no-gitconfig included');
  });
});

describe('structural_search arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders sg run with pattern and language', () => {
    const spec = loader.load('structural_search');
    const args = engine.renderArgs(spec, {
      pattern: 'console.log($A)',
      language: 'ts',
      json_format: true
    });

    assert.ok(args.includes('run'), 'run subcommand');
    assert.ok(args.includes('--pattern'), 'pattern flag');
    assert.ok(args.includes('console.log($A)'), 'pattern value');
    assert.ok(args.includes('--lang'), 'lang flag');
    assert.ok(args.includes('ts'), 'language value');
    assert.ok(args.some(a => a === '--json'), 'json flag');
  });
});

describe('directory_jump arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders zoxide query with all and score', () => {
    const spec = loader.load('directory_jump');
    const args = engine.renderArgs(spec, { query: 'proj', all_matches: true });

    assert.ok(args.includes('query'), 'query subcommand');
    assert.ok(args.some(a => a === '--all'), 'all flag');
    assert.ok(args.some(a => a === '--score'), 'score flag');
    assert.ok(args.includes('proj'), 'query value');
  });
});

// ═══════════════════════════════════════════════════════════════
// Validation Tests (5)
// ═══════════════════════════════════════════════════════════════

describe('fixed skills validation', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('rejects benchmark_execution without command', () => {
    const spec = loader.load('benchmark_execution');
    assert.throws(() => engine.renderArgs(spec, {}), TemplateRenderError);
  });

  it('rejects file_view without file', () => {
    const spec = loader.load('file_view');
    assert.throws(() => engine.renderArgs(spec, {}), TemplateRenderError);
  });

  it('rejects github_api with invalid action', () => {
    const spec = loader.load('github_api');
    assert.throws(() => engine.renderArgs(spec, { action: 'invalid' }), TemplateRenderError);
  });

  it('rejects js_lint_fix with invalid action', () => {
    const spec = loader.load('js_lint_fix');
    assert.throws(
      () => engine.renderArgs(spec, { path: 'src', action: 'fly' }),
      TemplateRenderError
    );
  });

  it('rejects python_lint_fix with invalid output_format', () => {
    const spec = loader.load('python_lint_fix');
    assert.throws(
      () => engine.renderArgs(spec, { path: 'src', action: 'check', output_format: 'xml' }),
      TemplateRenderError
    );
  });
});

// ═══════════════════════════════════════════════════════════════
// Structural Integrity / Normalization Tests (10)
// ═══════════════════════════════════════════════════════════════

describe('benchmark_execution normalization', () => {
  it('parse_json produces benchmark results envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'benchmark_execution',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify({
        results: [{ mean: 1.23, stddev: 0.1, median: 1.2, min: 1.1, max: 1.3, times: [1.1, 1.3] }],
        command: 'echo test'
      }),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { command: 'echo test' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.results.length, 1);
    assert.strictEqual(output.results[0].mean, 1.23);
    assert.strictEqual(output.command, 'echo test');
  });

  it('parse_time_output handles fallback time -p format', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'benchmark_execution',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_time_output' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'real 1.234\nuser 0.800\nsys 0.400\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { command: 'sleep 1' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.results.length, 1);
    assert.strictEqual(output.results[0].mean, 1.234);
  });
});

describe('file_view normalization', () => {
  it('parse_plaintext_lines produces line objects', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'file_view',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_plaintext_lines' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'line one\nline two\nline three',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total_lines, 3);
    assert.strictEqual(output.lines[0].line_number, 1);
    assert.strictEqual(output.lines[0].content, 'line one');
    assert.strictEqual(output.lines[0].highlighted, true);
  });
});

describe('github_api normalization', () => {
  it('parse_json produces pr_list envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'github_api',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify([{ number: 1, title: 'Fix bug' }]),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'pr_list' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.action, 'pr_list');
    assert.deepStrictEqual(output.prs, [{ number: 1, title: 'Fix bug' }]);
  });

  it('parse_json produces repo_view envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'github_api',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify({ name: 'myrepo', owner: 'me' }),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'repo_view' });

    assert.strictEqual(output.status, 'ok');
    assert.deepStrictEqual(output.repo, { name: 'myrepo', owner: 'me' });
  });
});

describe('js_lint_fix normalization', () => {
  it('parse_json produces diagnostics envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'js_lint_fix',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify({
        diagnostics: [{ message: 'Unused var', severity: 'error' }],
        files_processed: 1,
        errors: 1,
        warnings: 0,
        fixed: 0
      }),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'lint' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.diagnostics.length, 1);
    assert.strictEqual(output.errors, 1);
    assert.strictEqual(output.files_processed, 1);
  });
});

describe('process_list normalization', () => {
  it('parse_json produces process array envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'process_list',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify([
        { pid: 1, name: 'init', cpu: 0.1, memory: 10, user: 'root', time: '00:01', command: '/sbin/init' }
      ]),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total, 1);
    assert.strictEqual(output.processes[0].pid, 1);
    assert.strictEqual(output.processes[0].name, 'init');
  });

  it('parse_ps_output handles ps aux fallback', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'process_list',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_ps_output' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND\nroot         1  0.1  0.0   1234   567 ?        Ss   00:00   00:01 /sbin/init\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total, 1);
    assert.strictEqual(output.processes[0].command, '/sbin/init');
  });
});

describe('python_lint_fix normalization', () => {
  it('parse_json produces ruff diagnostics envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'python_lint_fix',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify([
        { file: 'main.py', line: 5, column: 10, code: 'E501', message: 'Line too long' }
      ]),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'check' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.diagnostics.length, 1);
    assert.strictEqual(output.diagnostics[0].code, 'E501');
  });

  it('parse_flake8_output handles flake8 fallback', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'python_lint_fix',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_flake8_output' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: 'main.py:5:10: E501 Line too long\nmain.py:8:1: W293 blank line contains whitespace\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'check' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.diagnostics.length, 2);
    assert.strictEqual(output.diagnostics[0].line, 5);
  });
});

describe('rust_test normalization', () => {
  it('parse_json produces test results envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'rust_test',
      inputs: {},
      normalization: {
        input_format: 'ndjson',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '{"type":"test","name":"it_works","status":"passed","duration":0.012}\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, { action: 'run' });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.tests.length, 1);
    assert.strictEqual(output.tests[0].name, 'it_works');
    assert.strictEqual(output.tests[0].status, 'passed');
    assert.strictEqual(output.summary.passed, 1);
  });
});

describe('semantic_diff normalization', () => {
  it('parse_plaintext_lines produces diff envelope with stats', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'semantic_diff',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_plaintext_lines' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.stats.insertions, 1);
    assert.strictEqual(output.stats.deletions, 1);
    assert.ok(output.raw_output.includes('old'));
  });
});

describe('structural_search normalization', () => {
  it('parse_json produces match envelope', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'structural_search',
      inputs: {},
      normalization: {
        input_format: 'json',
        pipeline: [{ step: 'parse_json' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: JSON.stringify([
        { file: 'src/main.ts', line: 10, column: 5, text: 'console.log(x)' }
      ]),
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.total, 1);
    assert.strictEqual(output.matches[0].file, 'src/main.ts');
    assert.strictEqual(output.matches[0].line, 10);
  });
});

describe('directory_jump normalization', () => {
  it('parse_plaintext_lines produces directory matches', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'directory_jump',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_plaintext_lines' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '/home/user/projects\n/var/log\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.matches.length, 2);
    assert.strictEqual(output.matches[0].path, '/home/user/projects');
    assert.strictEqual(output.selected, '/home/user/projects');
  });

  it('parses zoxide score format', () => {
    const pipeline = new NormalizationPipeline();
    const spec = {
      name: 'directory_jump',
      inputs: {},
      normalization: {
        input_format: 'plaintext',
        pipeline: [{ step: 'parse_plaintext_lines' }, { step: 'assemble_output' }]
      }
    };
    const result = new ExecutionResult({
      exitCode: 0,
      stdout: '50.0 /home/user/project\n',
      stderr: '',
      timedOut: false
    });
    const output = pipeline.run(spec, result, {});

    assert.strictEqual(output.matches[0].score, 50.0);
    assert.strictEqual(output.matches[0].path, '/home/user/project');
  });
});

// ═══════════════════════════════════════════════════════════════
// Runtime Integration Tests (5)
// ═══════════════════════════════════════════════════════════════

describe('fixed skills runtime integration', () => {
  it('loads all 10 fixed skills', () => {
    const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
    const names = [
      'benchmark_execution',
      'file_view',
      'github_api',
      'js_lint_fix',
      'process_list',
      'python_lint_fix',
      'rust_test',
      'semantic_diff',
      'structural_search',
      'directory_jump'
    ];

    for (const name of names) {
      const spec = loader.load(name);
      assert.strictEqual(spec.name, name, `${name} loaded correctly`);
      assert.ok(spec.execution, `${name} has execution config`);
      assert.ok(spec.normalization, `${name} has normalization config`);
      assert.ok(spec.resilience?.fallback, `${name} has fallback config`);
    }
  });

  it('lists all 10 fixed skills in runtime', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);

    assert.ok(names.includes('benchmark_execution'));
    assert.ok(names.includes('file_view'));
    assert.ok(names.includes('github_api'));
    assert.ok(names.includes('js_lint_fix'));
    assert.ok(names.includes('process_list'));
    assert.ok(names.includes('python_lint_fix'));
    assert.ok(names.includes('rust_test'));
    assert.ok(names.includes('semantic_diff'));
    assert.ok(names.includes('structural_search'));
    assert.ok(names.includes('directory_jump'));
  });

  it('returns validation error for missing required inputs at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('structural_search', {});

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('pattern'), 'mentions missing pattern');
  });

  it('returns validation error for invalid enum at runtime', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('rust_test', { action: 'fly' });

    assert.strictEqual(result.status, 'error');
    assert.strictEqual(result.error_type, 'invalid_argument');
    assert.ok(result.message.includes('action'), 'mentions invalid action');
  });

  it('executes directory_jump and returns structured result or command_not_found', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('directory_jump', { query: 'src' });

    // Either ok (if zoxide or find works) or error with proper envelope
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

console.log('All fixed skills tests defined.');
