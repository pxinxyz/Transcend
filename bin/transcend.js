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
  transcend <skill> [inputs...] [flags]

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

  // Initialize runtime
  const runtime = new TranscendRuntime({
    skillsDir: flags['skills-dir'] || './skills',
    timeoutMs: parseInt(flags.timeout || '60000', 10),
    debug: flags.debug || false
  });

  // --list
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

  // No skill specified
  if (!skillName) {
    console.error('Error: No skill specified. Run `transcend --list` to see available skills.');
    process.exit(1);
  }

  // --dry-run: just render args
  if (flags['dry-run']) {
    const { NunjucksEngine } = require('../lib/NunjucksEngine.js');
    const { SkillLoader } = require('../lib/SkillLoader.js');
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

    // Positional argument: first one is the skill name
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
