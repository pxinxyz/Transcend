#!/usr/bin/env node
/**
 * transcend — CLI entry point for the Transcend skill runtime.
 *
 * Usage:
 *   transcend <skill> [inputs...]
 *   transcend universal_search --pattern="fetchUser" --path="./src" --file_types=ts,js
 *   transcend codebase_analysis --path="." --granularity=per_language
 *   transcend find_replace --pattern="old" --replacement="new" --files="a.ts,b.ts"
 *
 * Commands:
 *   list                  Display all skills in a formatted table
 *   show <skill>          Display full JSON spec with pretty-printing
 *   validate              Check all skill specs for structural integrity
 *   test                  Run npm test
 *
 * Global flags:
 *   --skills-dir <path>   Directory containing .skill.json files (default: ./skills)
 *   --timeout <ms>        Global timeout in milliseconds (default: 60000)
 *   --debug               Enable debug logging
 *   --json                Output raw JSON (default: pretty print)
 *   --chain <skill>       Chain to a second skill after the first completes
 *   --dry-run             Show rendered args without executing
 *   --list                List all available skills and exit
 *   --version             Show version and exit
 *   --help                Show this help message
 */

import { createRequire } from 'module';
import { TranscendRuntime } from '../lib/TranscendRuntime.js';
import { ChainRouter } from '../lib/ChainRouter.js';
import { SkillLoader, SkillValidationError } from '../lib/SkillLoader.js';
import { SkillRegistry } from '../lib/SkillRegistry.js';
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const defaultSkillsDir = path.join(__dirname, '..', 'skills');

const require = createRequire(import.meta.url);
const pkg = require('../package.json');

const HELP = `
Transcend — Agent-Native CLI Skill Framework (v${pkg.version})

Usage:
  transcend <command> [options]
  transcend <skill> [inputs...] [flags]

Commands:
  list                  Display all skills in a formatted table
  show <skill>          Display full JSON spec with pretty-printing
  validate              Check all skill specs for structural integrity
  test                  Run npm test

Execute a skill:
  transcend universal_search --pattern="fetchUser" --path="./src"
  transcend codebase_analysis --path="." --granularity=summary
  transcend find_replace --pattern="old" --replacement="new" --files="a.ts,b.ts"

Global flags:
  --skills-dir <path>   Skill specs directory (default: ./skills)
  --timeout <ms>        Execution timeout (default: 60000)
  --debug               Enable verbose debug output
  --json                Output as JSON (default: pretty)
  --chain <skill>       Chain to downstream skill
  --dry-run             Show rendered args without executing
  --list                List available skills
  --version             Show version
  --help                Show this help

Input syntax:
  --key=value           String/boolean/number
  --key                 Boolean true
  --key=a,b,c           Array (comma-separated)
  --no-key              Boolean false (prefix with --no-)

Examples:
  # Search for "TODO" in TypeScript files
  transcend universal_search --pattern="TODO" --file_types=ts

  # Analyze codebase health
  transcend codebase_analysis --path="./my-project" --cocomo

  # Chain: search then replace (preview first)
  transcend universal_search --pattern="oldFunc" --path="./src" --chain=find_replace --replacement="newFunc" --dry-run
`;

const COMMANDS = ['list', 'show', 'validate', 'test'];

function main() {
  const args = process.argv.slice(2);

  // No args → help
  if (args.length === 0) {
    console.log(HELP);
    process.exit(0);
  }

  // Parse global flags and extract skill + inputs
  const { skillName, inputs, flags } = parseArgs(args);

  // --help
  if (flags.help) {
    console.log(HELP);
    process.exit(0);
  }

  // --version
  if (flags.version) {
    console.log(pkg.version);
    process.exit(0);
  }

  // Determine if first positional arg is a built-in command
  const firstPositional = args.find(a => !a.startsWith('--'));
  const isCommand = firstPositional && COMMANDS.includes(firstPositional);

  // Initialize runtime
  const runtime = new TranscendRuntime({
    skillsDir: flags['skills-dir'] || './skills',
    timeoutMs: parseInt(flags.timeout || '60000', 10),
    debug: flags.debug || false
  });

  // --list (legacy flag)
  if (flags.list) {
    const skills = runtime.listSkills();
    console.log('Available skills:\n');
    for (const s of skills) {
      const status = s.stability === 'stable' ? '✓' : s.stability === 'beta' ? 'β' : '⚑';
      console.log(`  ${status} ${s.name} ${s.version}`);
      console.log(`    ${s.description}`);
      console.log(`    category: ${s.category || 'general'}`);
      console.log();
    }
    process.exit(0);
  }

  // Built-in commands
  if (isCommand) {
    switch (firstPositional) {
      case 'list':
        return cmdList(runtime);
      case 'show':
        return cmdShow(runtime, args);
      case 'validate':
        return cmdValidate(runtime);
      case 'test':
        return cmdTest();
      default:
        console.error(`Error: Unknown command: ${firstPositional}`);
        process.exit(1);
    }
  }

  // No skill specified
  if (!skillName) {
    console.error('Error: No skill specified. Run `transcend --list` to see available skills.');
    process.exit(1);
  }

  // --dry-run: just render args
  if (flags['dry-run']) {
    const { NunjucksEngine } = await import('../lib/NunjucksEngine.js');
    const { SkillLoader } = await import('../lib/SkillLoader.js');
    const engine = new NunjucksEngine({ debug: flags.debug });
    const loader = new SkillLoader({ skillsDir: flags['skills-dir'] || defaultSkillsDir });

    try {
      const spec = loader.load(skillName);
      const rendered = engine.renderArgs(spec, inputs);
      console.log(`${spec.execution.command} ${rendered.map(a => a.includes(' ') ? `"${a}"` : a).join(' ')}`);
      process.exit(0);
    } catch (err) {
      console.error(`Error: ${err.message}`);
      process.exit(1);
    }
  }

  // Execute (and optionally chain)
  execute(runtime, skillName, inputs, flags);
}

function cmdList(runtime) {
  const registry = new SkillRegistry({ loader: runtime.loader });
  const skills = registry.getAll();

  // Compute column widths
  const nameWidth = Math.max(4, ...skills.map(s => s.name.length));
  const versionWidth = Math.max(7, ...skills.map(s => (s.version || '').length));
  const stabilityWidth = Math.max(9, ...skills.map(s => (s.stability || '').length));
  const categoryWidth = Math.max(8, ...skills.map(s => (s.category || 'general').length));

  const header = `${'NAME'.padEnd(nameWidth)}  ${'VERSION'.padEnd(versionWidth)}  ${'STABILITY'.padEnd(stabilityWidth)}  ${'CATEGORY'.padEnd(categoryWidth)}  DESCRIPTION`;
  const line = '-'.repeat(header.length);

  console.log(header);
  console.log(line);

  for (const s of skills) {
    const name = s.name.padEnd(nameWidth);
    const version = (s.version || '').padEnd(versionWidth);
    const stability = (s.stability || '').padEnd(stabilityWidth);
    const category = (s.category || 'general').padEnd(categoryWidth);
    const desc = (s.description || '').split('\n')[0];
    console.log(`${name}  ${version}  ${stability}  ${category}  ${desc}`);
  }

  console.log(`\nTotal: ${skills.length} skill(s)`);
  process.exit(0);
}

function cmdShow(runtime, args) {
  const showIndex = args.indexOf('show');
  const skillName = args[showIndex + 1];

  if (!skillName || skillName.startsWith('--')) {
    console.error('Error: No skill specified for show. Usage: transcend show <skill>');
    process.exit(1);
  }

  try {
    const spec = runtime.getSpec(skillName);
    console.log(JSON.stringify(spec, null, 2));
    process.exit(0);
  } catch (err) {
    console.error(`Error: ${err.message}`);
    process.exit(1);
  }
}

function cmdValidate(runtime) {
  const registry = new SkillRegistry({ loader: runtime.loader });
  const skills = registry.getAll();
  let errors = 0;

  for (const spec of skills) {
    try {
      runtime.loader.reload(spec.name);
    } catch (err) {
      errors++;
      console.error(`✗ ${spec.name}: ${err.message}`);
    }
  }

  if (errors === 0) {
    console.log(`✓ All ${skills.length} skill spec(s) are structurally valid.`);
    process.exit(0);
  } else {
    console.error(`\n✗ ${errors} skill spec(s) failed validation.`);
    process.exit(1);
  }
}

function cmdTest() {
  const child = spawn('npm', ['test'], {
    stdio: 'inherit',
    shell: process.platform === 'win32',
    cwd: path.join(__dirname, '..')
  });

  child.on('exit', code => {
    process.exit(code ?? 0);
  });

  child.on('error', err => {
    console.error(`Failed to run npm test: ${err.message}`);
    process.exit(1);
  });
}

async function execute(runtime, skillName, inputs, flags) {
  try {
    let result;

    if (flags.chain) {
      const router = new ChainRouter(runtime, { debug: flags.debug });
      const chainInputs = {};

      // Pass through additional inputs for the downstream skill
      if (inputs.replacement) chainInputs.replacement = inputs.replacement;
      if (inputs.dry_run) chainInputs.dry_run = inputs.dry_run;
      if (inputs.pattern) chainInputs.pattern = inputs.pattern;

      result = await router.chain(skillName, inputs, flags.chain, chainInputs);
    } else {
      result = await runtime.execute(skillName, inputs);
    }

    // Output
    if (flags.json) {
      console.log(JSON.stringify(result));
    } else {
      prettyPrint(result);
    }

    // Exit with non-zero if error
    if (result.status === 'error') {
      process.exit(result.exit_code || 1);
    }

  } catch (err) {
    console.error(`Runtime error: ${err.message}`);
    if (flags.debug) {
      console.error(err.stack);
    }
    process.exit(1);
  }
}

function parseArgs(args) {
  const inputs = {};
  const flags = {};
  let skillName = null;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];

    // --flag or --key=value
    if (arg.startsWith('--')) {
      const eqIdx = arg.indexOf('=');
      let key, value;

      if (eqIdx >= 0) {
        key = arg.slice(2, eqIdx);
        value = arg.slice(eqIdx + 1);
      } else {
        key = arg.slice(2);
        // Check for --no-prefix (boolean false)
        if (key.startsWith('no-')) {
          key = key.slice(3);
          value = false;
        } else {
          value = true;  // --flag without value = true
        }
      }

      // Parse arrays (comma-separated)
      if (typeof value === 'string' && value.includes(',')) {
        value = value.split(',').map(s => s.trim());
      }

      // Convert numbers
      if (typeof value === 'string' && /^-?\d+$/.test(value)) {
        value = parseInt(value, 10);
      }

      // Known global flags
      const globalFlags = ['skills-dir', 'timeout', 'debug', 'json', 'chain', 'dry-run', 'list', 'version', 'help'];
      if (globalFlags.includes(key)) {
        flags[key] = value;
      } else {
        inputs[key] = value;
      }
      continue;
    }

    // Positional argument: first one is the skill name, unless it's a built-in command
    if (!skillName && !arg.startsWith('-')) {
      skillName = arg;
    }
  }

  return { skillName, inputs, flags };
}

function prettyPrint(result) {
  if (result.status === 'ok') {
    console.log('✓ Success');
    console.log();

    // Universal search output
    if (result.total_matches !== undefined) {
      console.log(`  Matches: ${result.total_matches} across ${result.files_with_matches} files`);
      if (result.truncated) console.log('  ⚠ Results were truncated');
      if (result.used_fallback) console.log('  ⚠ Used fallback tool');
      if (result.execution_ms) console.log(`  Time: ${result.execution_ms}ms`);
      console.log();

      if (result.matches?.length > 0) {
        for (const m of result.matches.slice(0, 20)) {
          console.log(`  ${m.file}:${m.line_number}  ${m.match_text}`);
          if (m.line_text) console.log(`    ${m.line_text.trim()}`);
        }
        if (result.matches.length > 20) {
          console.log(`  ... and ${result.matches.length - 20} more`);
        }
      }
    }

    // Codebase analysis output
    if (result.total_files !== undefined) {
      console.log(`  Files: ${result.total_files}`);
      console.log(`  Lines: ${result.total_lines} (code: ${result.code_lines}, comments: ${result.comment_lines}, blank: ${result.blank_lines})`);
      console.log(`  Languages: ${result.estimated_languages}`);
      if (result.complexity_score) console.log(`  Complexity: ${result.complexity_score}`);
      if (result.cocomo) {
        console.log(`  COCOMO: ${result.cocomo.effort_months.toFixed(1)} person-months, ${result.cocomo.schedule_months.toFixed(1)} months, ~$${result.cocomo.cost_usd.toLocaleString()}`);
      }
      if (result.truncated) console.log('  ⚠ Results were truncated');
      console.log();

      if (result.languages?.length > 0) {
        console.log('  Language breakdown:');
        for (const lang of result.languages) {
          console.log(`    ${lang.name}: ${lang.files} files, ${lang.code} lines (${lang.code_percentage}%)`);
        }
      }
    }

    // Find/replace output
    if (result.total_files_processed !== undefined) {
      console.log(`  Files processed: ${result.total_files_processed}`);
      console.log(`  Replacements: ${result.total_replacements} in ${result.files_modified} files`);
      if (result.dry_run) console.log('  (dry run — no changes written)');
      if (result.truncated) console.log('  ⚠ File list was truncated');
      console.log();

      if (result.changes?.length > 0) {
        for (const change of result.changes.slice(0, 10)) {
          const icon = change.modified ? '✎' : ' ';
          console.log(`  ${icon} ${change.file}: ${change.replacements_count} replacements`);
          if (change.preview) {
            for (const p of change.preview) {
              console.log(`    - ${p.original_line?.trim()}`);
              console.log(`    + ${p.replaced_line?.trim()}`);
            }
          }
        }
      }
    }

    if (result.warnings?.length > 0) {
      console.log('\n  Warnings:');
      for (const w of result.warnings) {
        console.log(`    ⚠ ${w}`);
      }
    }
  } else {
    console.log(`✗ Error: ${result.error_type}`);
    console.log(`  ${result.message}`);
    if (result.stderr) {
      console.log(`  stderr: ${result.stderr}`);
    }
  }
}

main();
