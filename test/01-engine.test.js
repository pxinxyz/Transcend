/**
 * Tests for the Nunjucks template engine — the heart of Transcend.
 * Verifies that skill specs render to correct CLI arguments.
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { NunjucksEngine, TemplateRenderError } from '../lib/NunjucksEngine.js';
import { SkillLoader } from '../lib/SkillLoader.js';

const TEST_DIR = new URL('.', import.meta.url).pathname;
const SKILLS_DIR = TEST_DIR.replace('/test/', '/skills/');

// ── Universal Search Tests ─────────────────────────────────

describe('universal_search arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine({ debug: false });

  it('renders basic search with defaults', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, { pattern: 'hello', path: '.' });

    assert.ok(args.includes('--json'), 'should include --json');
    assert.ok(args.includes('--case-sensitive'), 'case-sensitive by default');
    assert.ok(args.includes('hello'), 'pattern included');
    assert.ok(args.includes('.'), 'path included');
    assert.ok(args.includes('--'), 'separator included');
  });

  it('renders case-insensitive search', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, { pattern: 'Hello', ignore_case: true });

    assert.ok(args.includes('--ignore-case'), 'ignore-case flag');
    assert.ok(!args.includes('--case-sensitive'), 'no case-sensitive flag');
  });

  it('renders literal string search', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, { pattern: '(foo)', literal: true });

    assert.ok(args.includes('--fixed-strings'), 'fixed-strings flag');
    assert.ok(args.includes('(foo)'), 'literal pattern preserved');
  });

  it('renders word boundary search', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, { pattern: 'test', word_regexp: true });

    assert.ok(args.includes('--word-regexp'), 'word-regexp flag');
  });

  it('renders file type filters', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, {
      pattern: 'foo',
      file_types: ['rust', 'ts']
    });

    assert.ok(args.includes('--type=rust'), 'rust type filter');
    assert.ok(args.includes('--type=ts'), 'ts type filter');
  });

  it('renders ignore patterns', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, {
      pattern: 'foo',
      ignore_patterns: ['node_modules/', '*.log']
    });

    assert.ok(args.includes('--glob=!node_modules/'), 'ignore pattern 1');
    assert.ok(args.includes('--glob=!*.log'), 'ignore pattern 2');
  });

  it('renders context lines', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, {
      pattern: 'foo',
      context_before: 2,
      context_after: 3
    });

    assert.ok(args.includes('--before-context=2'), 'before context');
    assert.ok(args.includes('--after-context=3'), 'after context');
  });

  it('renders sorted results', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, { pattern: 'foo', sort: 'path' });

    assert.ok(args.includes('--sort=path'), 'sort flag');
  });

  it('omits conditional flags when false', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, {
      pattern: 'foo',
      hidden: false,
      no_ignore: false,
      follow_symlinks: false
    });

    assert.ok(!args.includes('--hidden'), 'no hidden flag');
    assert.ok(!args.includes('--no-ignore'), 'no no-ignore flag');
    assert.ok(!args.includes('--follow'), 'no follow flag');
  });

  it('renders all flags when true', () => {
    const spec = loader.load('universal_search');
    const args = engine.renderArgs(spec, {
      pattern: 'foo',
      hidden: true,
      no_ignore: true,
      follow_symlinks: true
    });

    assert.ok(args.includes('--hidden'), 'hidden flag');
    assert.ok(args.includes('--no-ignore'), 'no-ignore flag');
    assert.ok(args.includes('--follow'), 'follow flag');
  });

  it('validates required pattern', () => {
    const spec = loader.load('universal_search');
    assert.throws(
      () => engine.renderArgs(spec, { path: '.' }),
      TemplateRenderError
    );
  });

  it('validates pattern type', () => {
    const spec = loader.load('universal_search');
    assert.throws(
      () => engine.renderArgs(spec, { pattern: 123 }),
      TemplateRenderError
    );
  });
});

// ── Codebase Analysis Tests ────────────────────────────────

describe('codebase_analysis arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders basic analysis', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, { path: '.' });

    assert.ok(args.includes('--format=json'), 'json format');
    assert.ok(args.includes('--sort=code'), 'default sort');
    assert.ok(!args.includes('--by-file'), 'no by-file by default');
  });

  it('renders per-file granularity', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, { path: '.', granularity: 'per_file' });

    assert.ok(args.includes('--by-file'), 'by-file flag');
  });

  it('renders COCOMO option', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, {
      path: '.',
      cocomo: true,
      cocomo_project_type: 'embedded'
    });

    assert.ok(args.includes('--cocomo'), 'cocomo flag');
    assert.ok(args.includes('--cocomo-project-type=embedded'), 'cocomo type');
  });

  it('renders exclude dirs', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, {
      path: '.',
      exclude_dirs: ['node_modules', 'target']
    });

    assert.ok(args.includes('--exclude-dir=node_modules'), 'exclude node_modules');
    assert.ok(args.includes('--exclude-dir=target'), 'exclude target');
  });

  it('renders language filter', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, {
      path: '.',
      languages: ['Go', 'Rust']
    });

    assert.ok(args.includes('--include-lang=Go'), 'include Go');
    assert.ok(args.includes('--include-lang=Rust'), 'include Rust');
  });

  it('disables complexity when false', () => {
    const spec = loader.load('codebase_analysis');
    const args = engine.renderArgs(spec, { path: '.', complexity: false });

    assert.ok(args.includes('--no-complexity'), 'no-complexity flag');
  });
});

// ── Find & Replace Tests ───────────────────────────────────

describe('find_replace arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders basic find-replace', () => {
    const spec = loader.load('find_replace');
    const args = engine.renderArgs(spec, {
      pattern: 'old',
      replacement: 'new',
      files: ['a.ts', 'b.ts']
    });

    assert.ok(args.includes('find_replace.cjs'), 'wrapper script');
    assert.ok(args.includes('--pattern=old'), 'pattern');
    assert.ok(args.includes('--replacement=new'), 'replacement');
  });

  it('renders with literal mode', () => {
    const spec = loader.load('find_replace');
    const args = engine.renderArgs(spec, {
      pattern: '(foo)',
      replacement: 'bar',
      files: ['a.ts'],
      literal: true
    });

    assert.ok(args.includes('--literal'), 'literal flag');
  });

  it('renders dry-run mode', () => {
    const spec = loader.load('find_replace');
    const args = engine.renderArgs(spec, {
      pattern: 'old',
      replacement: 'new',
      files: ['a.ts'],
      dry_run: true
    });

    assert.ok(args.includes('--dry-run'), 'dry-run flag');
  });

  it('validates required fields', () => {
    const spec = loader.load('find_replace');

    assert.throws(
      () => engine.renderArgs(spec, { replacement: 'new', files: ['a.ts'] }),
      TemplateRenderError,
      'missing pattern'
    );
    assert.throws(
      () => engine.renderArgs(spec, { pattern: 'old', files: ['a.ts'] }),
      TemplateRenderError,
      'missing replacement'
    );
  });

  it('validates files is an array', () => {
    const spec = loader.load('find_replace');
    assert.throws(
      () => engine.renderArgs(spec, { pattern: 'old', replacement: 'new', files: 'a.ts' }),
      TemplateRenderError
    );
  });
});

// ── Edge Cases ─────────────────────────────────────────────

describe('edge cases', () => {
  const engine = new NunjucksEngine();

  it('handles empty arrays in loops', () => {
    const spec = {
      inputs: {
        pattern: { type: 'string', required: true },
        file_types: { type: 'array', default: [] }
      },
      execution: {
        command: 'test',
        args: [
          '{{ pattern }}',
          '{% for ft in file_types %}--type={{ ft }}{% endfor %}'
        ],
        template_engine: 'nunjucks'
      }
    };

    const args = engine.renderArgs(spec, { pattern: 'foo' });
    assert.deepStrictEqual(args, ['foo']);
  });

  it('handles numeric inputs', () => {
    const spec = {
      inputs: {
        limit: { type: 'integer', default: 10 }
      },
      execution: {
        command: 'test',
        args: ['--limit={{ limit }}'],
        template_engine: 'nunjucks'
      }
    };

    const args = engine.renderArgs(spec, { limit: 42 });
    assert.ok(args.includes('--limit=42'));
  });

  it('handles conditional blocks', () => {
    const spec = {
      inputs: {
        verbose: { type: 'boolean', default: false }
      },
      execution: {
        command: 'test',
        args: [
          'base',
          '{% if verbose %}--verbose{% endif %}'
        ],
        template_engine: 'nunjucks'
      }
    };

    const argsOff = engine.renderArgs(spec, { verbose: false });
    assert.deepStrictEqual(argsOff, ['base']);

    const argsOn = engine.renderArgs(spec, { verbose: true });
    assert.deepStrictEqual(argsOn, ['base', '--verbose']);
  });
});

console.log('All Nunjucks engine tests defined.');
