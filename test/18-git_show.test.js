/**
 * Tests for the git_show skill — argument rendering, normalization pipeline,
 * auto-detection, diff handling, oneline fallback, and runtime integration.
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

describe('git_show arg rendering', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const engine = new NunjucksEngine();

  it('renders basic git show with defaults', () => {
    const spec = loader.load('git_show');
    const args = engine.renderArgs(spec, {});

    assert.ok(args.includes('show'), 'includes show subcommand');
    assert.ok(args.some(a => a.includes('>>>COMMIT_START<<<')), 'structured format');
    assert.ok(args.includes('--no-patch'), 'no patch by default');
    assert.ok(!args.includes('--oneline'), 'not oneline by default');
  });

  it('renders with object and include_diff', () => {
    const spec = loader.load('git_show');
    const args = engine.renderArgs(spec, {
      object: 'abc1234',
      include_diff: true
    });

    assert.ok(args.includes('abc1234'), 'includes object');
    assert.ok(args.includes('--patch'), 'patch flag');
    assert.ok(!args.includes('--no-patch'), 'no no-patch');
  });

  it('renders with include_stats', () => {
    const spec = loader.load('git_show');
    const args = engine.renderArgs(spec, { include_stats: true });

    assert.ok(args.includes('--numstat'), 'numstat flag');
  });

  it('renders oneline format', () => {
    const spec = loader.load('git_show');
    const args = engine.renderArgs(spec, { format: 'oneline' });

    assert.ok(args.includes('--oneline'), 'oneline flag');
    assert.ok(!args.some(a => a.includes('>>>COMMIT_START<<<')), 'no structured format');
  });
});

// ── Normalization Pipeline Tests ───────────────────────────

describe('git_show normalization pipeline', () => {
  const loader = new SkillLoader({ skillsDir: SKILLS_DIR });
  const normalizer = new NormalizationPipeline();

  function mockResult(stdout, exitCode = 0, args = ['show']) {
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

  it('parses commit with full metadata', () => {
    const stdout = `>>>COMMIT_START<<<
abc123def456abc123def456abc123def456abc123
abc1234
Initial commit

>>>META<<<
Alice
alice@example.com
Bob
bob@example.com
2024-01-15T09:30:00+00:00
3 days ago
2024-01-15T09:30:00+00:00
>>>COMMIT_END<<<`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.type, 'commit');
    assert.strictEqual(output.hash, 'abc123def456abc123def456abc123def456abc123');
    assert.strictEqual(output.short_hash, 'abc1234');
    assert.strictEqual(output.subject, 'Initial commit');
    assert.strictEqual(output.author, 'Alice');
    assert.strictEqual(output.committer, 'Bob');
    assert.strictEqual(output.date, '2024-01-15T09:30:00+00:00');
    assert.strictEqual(output.date_relative, '3 days ago');
    assert.strictEqual(output.committer_date, '2024-01-15T09:30:00+00:00');
  });

  it('parses commit with multi-line body', () => {
    const stdout = `>>>COMMIT_START<<<
1111111111111111111111111111111111111111
1111111
Add feature X

This adds the long-awaited feature.
It includes tests.

Closes #42.
>>>META<<<
Dana
dana@example.com
Dana
dana@example.com
2024-03-10T15:45:00+00:00
1 month ago
2024-03-10T15:45:00+00:00
>>>COMMIT_END<<<`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.type, 'commit');
    assert.strictEqual(output.subject, 'Add feature X');
    assert.ok(output.body.includes('long-awaited feature'));
    assert.ok(output.body.includes('Closes #42.'));
  });

  it('parses commit with stats', () => {
    const stdout = `>>>COMMIT_START<<<
2222222222222222222222222222222222222222
2222222
Update readme

>>>META<<<
Eve
eve@example.com
Eve
eve@example.com
2024-04-20T08:00:00+00:00
5 days ago
2024-04-20T08:00:00+00:00
>>>COMMIT_END<<<
10	5	README.md
3	2	src/index.js
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { include_stats: true });

    assert.strictEqual(output.type, 'commit');
    assert.ok(output.stats, 'has stats');
    assert.strictEqual(output.stats.files, 2);
    assert.strictEqual(output.stats.insertions, 13);
    assert.strictEqual(output.stats.deletions, 7);
  });

  it('parses tree entries', () => {
    const stdout = `040000 tree abc123def456abc123def456abc123def456abc1	src
100644 blob def789abc012def789abc012def789abc012def789	README.md
100644 blob aaa111bbb222ccc333ddd444eee555fff666aaa777	package.json
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.type, 'tree');
    assert.ok(Array.isArray(output.entries));
    assert.strictEqual(output.entries.length, 3);
    assert.strictEqual(output.total_entries, 3);

    const first = output.entries[0];
    assert.strictEqual(first.mode, '040000');
    assert.strictEqual(first.type, 'tree');
    assert.strictEqual(first.hash, 'abc123def456abc123def456abc123def456abc1');
    assert.strictEqual(first.path, 'src');

    const second = output.entries[1];
    assert.strictEqual(second.mode, '100644');
    assert.strictEqual(second.type, 'blob');
    assert.strictEqual(second.path, 'README.md');
  });

  it('parses blob with truncation', () => {
    const stdout = `line one
line two
line three
line four
line five`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { max_lines_per_file: 3 });

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.type, 'blob');
    assert.strictEqual(output.content, stdout);
    assert.strictEqual(output.size, stdout.length);
    assert.strictEqual(output.lines.length, 3);
    assert.strictEqual(output.lines[0], 'line one');
    assert.strictEqual(output.truncated, true);
  });

  it('parses annotated tag metadata', () => {
    const stdout = `tag v1.0.0
Tagger: Alice <alice@example.com>
Date:   Mon Jan 1 12:00:00 2024 +0000

First release

commit abc123def456abc123def456abc123def456abc1
Author: Bob <bob@example.com>
Date:   Sun Dec 31 12:00:00 2023 +0000

    Initial commit
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.type, 'tag');
    assert.strictEqual(output.name, 'v1.0.0');
    assert.strictEqual(output.tagger, 'Alice <alice@example.com>');
    assert.ok(output.date);
    assert.strictEqual(output.message, 'First release');
    assert.strictEqual(output.object, 'abc123def456abc123def456abc123def456abc1');
  });

  it('includes diff when include_diff is true', () => {
    const stdout = `>>>COMMIT_START<<<
3333333333333333333333333333333333333333
3333333
Refactor utils

>>>META<<<
Frank
frank@example.com
Frank
frank@example.com
2024-05-01T10:00:00+00:00
1 day ago
2024-05-01T10:00:00+00:00
>>>COMMIT_END<<<
diff --git a/src/utils.js b/src/utils.js
index abc..def 100644
--- a/src/utils.js
+++ b/src/utils.js
@@ -1,3 +1,4 @@
 line one
 line two
+added line
 line three
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { include_diff: true });

    assert.strictEqual(output.type, 'commit');
    assert.ok(Array.isArray(output.diffs), 'has diffs array');
    assert.strictEqual(output.diffs.length, 1);
    assert.strictEqual(output.diffs[0].file, 'src/utils.js');
    assert.strictEqual(output.diffs[0].change_type, 'modified');
    assert.ok(Array.isArray(output.diffs[0].hunks));
    assert.strictEqual(output.diffs[0].hunks.length, 1);
    assert.strictEqual(output.diffs[0].hunks[0].lines.length, 4);

    const addedLine = output.diffs[0].hunks[0].lines.find(l => l.type === 'addition');
    assert.ok(addedLine);
    assert.strictEqual(addedLine.content, 'added line');
  });

  it('excludes diff when include_diff is false', () => {
    const stdout = `>>>COMMIT_START<<<
4444444444444444444444444444444444444444
4444444
Update docs

>>>META<<<
Grace
grace@example.com
Grace
grace@example.com
2024-06-01T08:00:00+00:00
2 days ago
2024-06-01T08:00:00+00:00
>>>COMMIT_END<<<
diff --git a/README.md b/README.md
index abc..def 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, { include_diff: false });

    assert.strictEqual(output.type, 'commit');
    assert.strictEqual(output.diffs, undefined);
  });

  it('parses oneline fallback via git_show_short', () => {
    const spec = {
      name: 'git_show',
      inputs: { max_lines_per_file: { default: 500 } },
      normalization: {
        input_format: 'plaintext',
        pipeline: [
          { step: 'git_show_short' },
          { step: 'assemble_output' }
        ]
      }
    };
    const result = mockResult('abc1234 Fix typo');
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.status, 'ok');
    assert.strictEqual(output.type, 'commit');
    assert.strictEqual(output.short_hash, 'abc1234');
    assert.strictEqual(output.subject, 'Fix typo');
    assert.strictEqual(output.hash, '');
    assert.strictEqual(output.author, '');
  });

  it('auto-detects commit type from markers', () => {
    const stdout = `>>>COMMIT_START<<<
5555555555555555555555555555555555555555
5555555
Auto-detected commit

>>>META<<<
Henry
henry@example.com
Henry
henry@example.com
2024-07-01T10:00:00+00:00
1 week ago
2024-07-01T10:00:00+00:00
>>>COMMIT_END<<<`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.type, 'commit');
    assert.strictEqual(output.hash, '5555555555555555555555555555555555555555');
  });

  it('auto-detects tree type from entries', () => {
    const stdout = `100644 blob aaa111bbb222ccc333ddd444eee555fff666aaa777	index.js
040000 tree bbb222ccc333ddd444eee555fff666aaa777bbb888	lib
`;

    const spec = loader.load('git_show');
    const result = mockResult(stdout);
    const output = normalizer.run(spec, result, {});

    assert.strictEqual(output.type, 'tree');
    assert.strictEqual(output.entries.length, 2);
  });
});

// ── Runtime Integration Tests ──────────────────────────────

describe('git_show runtime integration', () => {
  it('is available in runtime skill list', () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const skills = rt.listSkills();
    const names = skills.map(s => s.name);
    assert.ok(names.includes('git_show'), 'git_show is listed');
  });

  it('executes git_show on this repo HEAD', async () => {
    const rt = createRuntime({ skillsDir: SKILLS_DIR });
    const result = await rt.execute('git_show', {
      object: 'HEAD',
      format: 'structured'
    });

    assert.strictEqual(result.status, 'ok', `expected ok but got: ${result.message}`);
    assert.strictEqual(result.type, 'commit');
    assert.ok(result.hash, 'has hash');
    assert.ok(result.short_hash, 'has short_hash');
    assert.ok(result.subject, 'has subject');
    assert.ok(result.author, 'has author');
    assert.ok(result.committer, 'has committer');
    assert.ok(result.date, 'has date');
  });
});

console.log('All git_show tests defined.');
