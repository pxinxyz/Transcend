/**
 * NormalizationPipeline — Transforms raw CLI output into typed JSON contracts.
 *
 * Each skill defines a pipeline: an ordered list of steps that process the
 * raw subprocess result into the skill's declared output schema. Steps include:
 *
 *   - filter_message_types  (discard noise, keep relevant records)
 *   - extract_matches       (map raw fields to transcend schema)
 *   - attach_context        (associate related records)
 *   - extract_summary       (aggregate statistics)
 *   - handle_no_matches     (emit zero-result envelopes)
 *   - truncate_matches      (enforce max_matches cap)
 *   - detect_truncation     (flag incomplete results)
 *   - parse_stderr_errors   (classify error messages)
 *   - assemble_output       (build final envelope)
 *   - parse_json            (parse JSON/NDJSON)
 *   - extract_languages     (map language-level data)
 *   - compute_totals        (aggregate sums)
 *   - compute_percentages   (calculate ratios)
 *   - extract_cocomo        (aggregate COCOMO estimates)
 *   - truncate_files        (enforce per-language file caps)
 *   - strip_null_fields     (remove null values for token density)
 *   - validate_schema       (ensure output matches spec)
 *   - strip_empty_replacements (omit empty arrays)
 *   - normalize_paths       (standardize path formatting)
 *
 * The pipeline is pure: it takes ExecutionResult + skill spec + inputs,
 * and returns either a success envelope or an error envelope.
 */

import { basename, normalize } from 'path';

export class NormalizationPipeline {
  constructor(options = {}) {
    this.debug = options.debug ?? false;
  }

  /**
   * Run the full normalization pipeline for a skill.
   *
   * @param {Object} spec — loaded skill specification
   * @param {ExecutionResult} result — raw subprocess result
   * @param {Object} inputs — merged user inputs (for caps, flags, etc.)
   * @returns {Object} — structured JSON envelope matching spec.outputs schema
   */
  run(spec, result, inputs) {
    const pipeline = spec.normalization?.pipeline || [];
    const maxMatches = inputs.max_matches ?? inputs.max_results ?? spec.inputs.max_matches?.default ?? spec.inputs.max_results?.default ?? 1000;

    // Pipeline state — mutable as we progress through steps
    const state = {
      spec,
      inputs,
      result,
      maxMatches,
      // Parsed data accumulates here
      matches: [],
      summary: {},
      languages: [],
      changes: [],
      results: [],
      errors: [],
      warnings: [],
      truncated: false,
      usedFallback: false,
      executionMs: null,
      // Final envelope (set by assemble_output)
      output: null,
      // Whether to skip remaining steps (set by handle_no_matches or handle_empty)
      done: false
    };

    // Determine input format (ndjson, json, plaintext)
    const inputFormat = spec.normalization?.input_format || 'ndjson';

    // Parse raw output into structured form
    if (inputFormat === 'ndjson') {
      state.parsed = result.parseNdjson();
    } else if (inputFormat === 'json') {
      state.parsed = result.parseJson();
    } else {
      state.parsed = { raw: result.stdout, lines: result.stdout.split('\n') };
    }

    if (this.debug) {
      console.error(`[NormalizationPipeline] Input format: ${inputFormat}`);
      console.error(`[NormalizationPipeline] Parsed ${Array.isArray(state.parsed) ? state.parsed.length : 'structured'} records`);
    }

    // Execute each pipeline step in order
    for (const step of pipeline) {
      if (state.done) break;

      const stepFn = STEP_REGISTRY[step.step];
      if (!stepFn) {
        if (this.debug) {
          console.error(`[NormalizationPipeline] Unknown step: "${step.step}" — skipping`);
        }
        continue;
      }

      try {
        if (this.debug) {
          console.error(`[NormalizationPipeline] Running step: ${step.step}`);
        }
        stepFn(state, step);
      } catch (err) {
        console.error(`[NormalizationPipeline] Step "${step.step}" failed: ${err.message}`);
        // Continue to next step unless it's critical
      }
    }

    // If no assemble_output step ran, build a basic envelope
    if (!state.output) {
      state.output = this._fallbackEnvelope(state, result);
    }

    return state.output;
  }

  /**
   * Build a minimal envelope when no assemble_output step was defined.
   */
  _fallbackEnvelope(state, result) {
    // If command not found, return error envelope
    if (result.commandNotFound) {
      return {
        status: 'error',
        error_type: 'command_not_found',
        message: `Command "${result.command}" not found in PATH`,
        exit_code: null,
        stderr: result.stderr,
        used_fallback: false
      };
    }

    // If timed out, return error envelope
    if (result.timedOut) {
      return {
        status: 'error',
        error_type: 'timeout',
        message: `Execution timed out after ${state.spec.execution.timeout_ms || 30000}ms`,
        exit_code: result.exitCode,
        stderr: result.stderr,
        used_fallback: false
      };
    }

    // If exit code indicates error, try to parse stderr
    if (result.exitCode !== 0 && result.exitCode !== 1) {
      return {
        status: 'error',
        error_type: 'unknown',
        message: result.stderr || `Process exited with code ${result.exitCode}`,
        exit_code: result.exitCode,
        stderr: result.stderr,
        used_fallback: false
      };
    }

    // Default: return raw data in a generic envelope
    return {
      status: 'ok',
      raw_output: result.stdout,
      exit_code: result.exitCode,
      warnings: []
    };
  }
}

// ============================================================
// STEP IMPLEMENTATIONS
// Each function receives (state, stepConfig) and mutates state.
// ============================================================

const STEP_REGISTRY = {

  // ── Universal Search Steps ───────────────────────────────

  filter_message_types(state, config) {
    const retain = config.retain || [];
    if (Array.isArray(state.parsed)) {
      state.parsed = state.parsed.filter(rec => retain.includes(rec.type));
    }
  },

  extract_matches(state, config) {
    const { from_type, mapping } = config;
    const matches = [];

    for (const rec of state.parsed) {
      if (rec.type !== from_type) continue;

      const match = {};
      for (const [outField, path] of Object.entries(mapping)) {
        match[outField] = resolvePath(rec, path);
      }

      // Apply transforms
      if (config.transforms?.trim_trailing_newlines && match.line_text) {
        match.line_text = match.line_text.replace(/\r?\n$/, '');
      }

      matches.push(match);
    }

    state.matches = matches;
  },

  attach_context(state, config) {
    // Context messages (type: "context") contain lines before/after matches.
    // Associate them with the nearest match by line proximity.
    const contextRecs = state.parsed.filter(r => r.type === 'context');
    if (!contextRecs.length) return;

    for (const ctx of contextRecs) {
      const ctxLines = ctx.data?.lines || [];
      const ctxLineNum = ctx.data?.line_number;

      // Find the nearest match
      let nearest = null;
      let minDist = Infinity;
      for (const m of state.matches) {
        const dist = Math.abs(m.line_number - ctxLineNum);
        if (dist < minDist) {
          minDist = dist;
          nearest = m;
        }
      }

      if (nearest) {
        if (!nearest.context_before) nearest.context_before = [];
        if (!nearest.context_after) nearest.context_after = [];

        // Determine if this is before or after the match
        if (ctxLineNum < nearest.line_number) {
          nearest.context_before.push(...ctxLines.map(l =>
            typeof l === 'string' ? l.replace(/\r?\n$/, '') : l
          ));
        } else {
          nearest.context_after.push(...ctxLines.map(l =>
            typeof l === 'string' ? l.replace(/\r?\n$/, '') : l
          ));
        }
      }
    }
  },

  extract_summary(state, config) {
    const summaryRec = state.parsed.find(r => r.type === 'summary');
    if (!summaryRec) return;

    const { mapping } = config;
    for (const [outField, expr] of Object.entries(mapping)) {
      if (typeof expr === 'string' && expr.includes('*')) {
        // Arithmetic expression like "data.elapsed_total.secs * 1000"
        state.summary[outField] = evaluateExpression(summaryRec, expr);
      } else {
        state.summary[outField] = resolvePath(summaryRec, expr);
      }
    }
  },

  handle_no_matches(state, config) {
    const exitCode = state.result.exitCode;
    const condition = config.condition || '';

    const noMatches =
      condition.includes('exit_code == 1') && exitCode === 1 ||
      condition.includes('matches array is empty') && state.matches.length === 0;

    if (noMatches) {
      state.output = {
        status: 'ok',
        total_matches: 0,
        files_searched: state.summary.files_searched || 0,
        files_with_matches: 0,
        truncated: false,
        matches: [],
        warnings: []
      };
      state.done = true;
    }
  },

  truncate_matches(state, config) {
    const limit = state.maxMatches;
    if (state.matches.length > limit) {
      state.matches = state.matches.slice(0, limit);
      state.truncated = true;
    }
  },

  detect_truncation(state, config) {
    const condition = config.condition || '';
    if (condition.includes('truncated not already set') && !state.truncated) {
      if (state.summary.total_matches > state.matches.length) {
        state.truncated = true;
      }
    }
    if (config.set && 'truncated' in config.set && !state.truncated) {
      state.truncated = config.set.truncated;
    }
  },

  parse_stderr_errors(state, config) {
    const exitCode = state.result.exitCode;
    if (exitCode !== 2) return;

    const stderr = state.result.stderr;
    const errorMapping = config.mapping?.error_type;
    if (!errorMapping) return;

    let detectedType = errorMapping.default || 'unknown';

    for (const [key, def] of Object.entries(errorMapping)) {
      if (key === 'default') continue;
      if (def.regex) {
        const re = new RegExp(def.regex, 'im');
        if (re.test(stderr)) {
          detectedType = def.yields || key;
          break;
        }
      }
    }

    state.output = {
      status: 'error',
      error_type: detectedType,
      message: stderr || `Process exited with code ${exitCode}`,
      exit_code: exitCode,
      stderr,
      used_fallback: false
    };
    state.done = true;
  },

  parse_jq_output(state, config) {
    const stdout = state.result.stdout.trim();
    if (!stdout) {
      state.results = [];
      return;
    }

    const results = [];
    let remaining = stdout;

    while (remaining.trim().length > 0) {
      let parsed = null;
      let parseEnd = 0;

      for (let i = 1; i <= remaining.length; i++) {
        const slice = remaining.slice(0, i).trim();
        if (!slice) continue;

        try {
          parsed = JSON.parse(slice);
          parseEnd = i;
          break;
        } catch {
          // Continue trying longer prefixes
        }
      }

      if (parsed !== null) {
        results.push(parsed);
        remaining = remaining.slice(parseEnd);
      } else {
        break;
      }
    }

    state.results = results;
  },

  truncate_results(state, config) {
    const limit = state.maxMatches;
    if (state.results.length > limit) {
      state.results = state.results.slice(0, limit);
      state.truncated = true;
    }
  },

  assemble_output(state, config) {
    // Skill-aware output assembly
    const specName = state.spec?.name;

    // ── JSON Query ───────────────────────────────────────────
    if (specName === 'json_query') {
      const output = {
        status: 'ok',
        total_results: state.results.length,
        truncated: state.truncated || false,
        results: state.results,
        warnings: state.warnings || []
      };
      state.output = output;
      return;
    }

    // ── Codebase Analysis ────────────────────────────────────
    if (specName === 'codebase_analysis' || (state.languages?.length > 0 && !state.matches.length)) {
      const output = {
        status: 'ok',
        total_files: state.total_files || 0,
        total_lines: state.total_lines || 0,
        code_lines: state.code_lines || 0,
        comment_lines: state.comment_lines || 0,
        blank_lines: state.blank_lines || 0,
        complexity_score: state.complexity_score ?? null,
        estimated_languages: state.estimated_languages || 0,
        cocomo: state.cocomo ?? null,
        truncated: state.truncated || false,
        languages: state.languages || [],
        warnings: state.warnings || []
      };
      if (output.complexity_score === null) delete output.complexity_score;
      if (output.cocomo === null) delete output.cocomo;
      state.output = output;
      return;
    }

    // ── Find & Replace ───────────────────────────────────────
    if (specName === 'find_replace' || state.total_files_processed !== undefined) {
      const changes = state.parsed?.changes || [];
      const output = {
        status: 'ok',
        total_files_processed: state.total_files_processed ?? changes.length,
        total_replacements: state.total_replacements || 0,
        files_modified: state.files_modified || 0,
        files_unchanged: state.files_unchanged || 0,
        truncated: state.truncated || false,
        changes: changes,
        warnings: state.warnings || []
      };
      state.output = output;
      return;
    }

    // ── Git Blame ──────────────────────────────────────────
    if (specName === 'git_blame') {
      const output = {
        status: 'ok',
        file: state.inputs.file || state.result.args?.slice(-1)[0] || '',
        total_lines: state.lines.length,
        lines: state.lines,
        truncated: state.truncated || false,
        warnings: state.warnings || []
      };
      state.output = output;
      return;
    }

    // ── Git Stash ──────────────────────────────────────────
    if (specName === 'git_stash') {
      const action = state.inputs.action || 'list';
      const output = {
        status: 'ok',
        action,
        total_stashes: state.stashes.length,
        stashes: state.stashes,
        truncated: state.truncated || false,
        warnings: state.warnings || []
      };
      if (action === 'push') output.created = true;
      if (action === 'apply') output.applied = true;
      if (action === 'pop') {
        output.applied = true;
        output.dropped = true;
      }
      if (action === 'drop') output.dropped = true;
      if (action === 'clear') output.cleared = true;
      if (action === 'branch') {
        output.branched = true;
        output.branch_name = state.inputs.branch_name || null;
      }
      state.output = output;
      return;
    }

    // ── Git Log ────────────────────────────────────────────
    if (specName === 'git_log' || state.commits) {
      const output = {
        status: 'ok',
        total_commits: state.commits.length,
        commits: state.commits,
        truncated: state.truncated || false,
        warnings: state.warnings || []
      };
      state.output = output;
      return;
    }

    // ── Git Branch ───────────────────────────────────────────
    if (specName === 'git_branch' || state.branches) {
      const output = {
        status: 'ok',
        current_branch: state.current_branch || null,
        branches: state.branches || [],
        total_branches: (state.branches || []).length,
        truncated: state.truncated || false,
        warnings: state.warnings || []
      };

      // Action-specific fields
      const action = state.inputs.action || 'list';
      if (action === 'create') {
        output.created = state.inputs.branch_name;
      } else if (action === 'delete' || action === 'force_delete') {
        output.deleted = state.inputs.branch_name;
      } else if (action === 'rename') {
        output.renamed_from = state.inputs.branch_name;
        output.renamed_to = state.inputs.new_branch_name;
      }

      // Omit null/undefined fields for token density
      if (output.current_branch === null) delete output.current_branch;
      if (output.created === undefined) delete output.created;
      if (output.deleted === undefined) delete output.deleted;
      if (output.renamed_from === undefined) delete output.renamed_from;
      if (output.renamed_to === undefined) delete output.renamed_to;

      state.output = output;
      return;
    }

    // ── Universal Search (default) ───────────────────────────
    const output = {
      status: 'ok',
      total_matches: state.matches.length,
      files_searched: state.summary.files_searched || state.matches.length,
      files_with_matches: new Set(state.matches.map(m => m.file)).size,
      truncated: state.truncated,
      matches: state.matches,
      warnings: state.warnings || []
    };

    if (state.summary.execution_ms) {
      output.execution_ms = Math.round(state.summary.execution_ms);
    }

    state.output = output;
  },

  // ── Codebase Analysis Steps ──────────────────────────────

  parse_json(state, config) {
    // Already parsed in run() based on input_format
    if (!state.parsed && state.result.stdout) {
      try {
        state.parsed = JSON.parse(state.result.stdout);
      } catch {
        state.parsed = [];
      }
    }
  },

  handle_empty(state, config) {
    const condition = config.condition || '';
    const isEmpty =
      Array.isArray(state.parsed) && state.parsed.length === 0 ||
      !state.parsed;

    if (isEmpty) {
      state.output = config.emit ? structuredClone(config.emit) : {
        status: 'ok',
        total_files: 0, total_lines: 0, code_lines: 0,
        comment_lines: 0, blank_lines: 0,
        complexity_score: null, estimated_languages: 0,
        cocomo: null, truncated: false,
        languages: [],
        warnings: ['No recognised source files found in path.']
      };
      state.done = true;
    }
  },

  extract_languages(state, config) {
    const { mapping } = config;
    const granularity = state.inputs.granularity || 'per_language';

    if (!Array.isArray(state.parsed)) return;

    state.languages = state.parsed.map(lang => {
      const out = {};
      for (const [field, src] of Object.entries(mapping)) {
        if (field === 'files_detail' && granularity !== 'per_file') {
          // Omit files_detail entirely unless granularity is per_file
          continue;
        }
        if (typeof src === 'string') {
          out[field] = resolveSccField(lang, src);
        }
      }

      // Compute lines as sum of Code + Comment + Blank
      out.lines = (out.code || 0) + (out.comments || 0) + (out.blanks || 0);

      // Handle files_detail for per_file granularity
      if (granularity === 'per_file' && lang.Files) {
        out.files_detail = lang.Files.map(f => ({
          path: normalize(f.Location || f.Path || f.Filename || ''),
          lines: (f.Code || 0) + (f.Comment || 0) + (f.Blank || 0),
          code: f.Code || 0,
          comments: f.Comment || 0,
          blanks: f.Blank || 0,
          complexity: f.Complexity || null
        }));
      }

      return out;
    });
  },

  normalize_paths(state, config) {
    for (const lang of state.languages) {
      if (lang.files_detail) {
        for (const f of lang.files_detail) {
          // Convert to POSIX relative paths
          f.path = f.path.replace(/\\/g, '/').replace(/^\.\//, '');
        }
      }
    }
  },

  compute_totals(state, config) {
    const specName = state.spec?.name;

    // ── Find & Replace totals ────────────────────────────────
    if (specName === 'find_replace' || (state.parsed && state.parsed.changes)) {
      const changes = state.parsed?.changes || [];
      state.total_files_processed = changes.length;
      state.total_replacements = changes.reduce((s, c) => s + (c.replacements_count || 0), 0);
      state.files_modified = changes.filter(c => c.modified).length;
      state.files_unchanged = changes.filter(c => !c.modified).length;
      return;
    }

    // ── Codebase Analysis totals ─────────────────────────────
    const { aggregates } = config;
    state.total_files = sum(state.languages, 'files');
    state.total_lines = sum(state.languages, 'lines');
    state.code_lines = sum(state.languages, 'code');
    state.comment_lines = sum(state.languages, 'comments');
    state.blank_lines = sum(state.languages, 'blanks');

    const complexities = state.languages.map(l => l.complexity).filter(c => c != null);
    state.complexity_score = complexities.length === state.languages.length
      ? complexities.reduce((a, b) => a + b, 0)
      : null;

    state.estimated_languages = state.languages.length;
  },

  compute_percentages(state, config) {
    for (const lang of state.languages) {
      lang.code_percentage = state.code_lines > 0
        ? Math.round((lang.code / state.code_lines) * 1000) / 10
        : 0.0;
    }
  },

  extract_cocomo(state, config) {
    if (!state.inputs.cocomo) {
      state.cocomo = null;
      return;
    }

    // Sum COCOMO across languages
    let effort = 0, schedule = 0, people = 0, cost = 0;
    let hasCocomo = false;

    for (const lang of state.parsed || []) {
      if (lang.Cocomo) {
        hasCocomo = true;
        effort += lang.Cocomo.Effort || 0;
        schedule = Math.max(schedule, lang.Cocomo.Months || 0);
        people = Math.max(people, lang.Cocomo.People || 0);
        cost += lang.Cocomo.Cost || 0;
      }
    }

    state.cocomo = hasCocomo ? {
      effort_months: effort,
      schedule_months: schedule,
      people_required: people,
      cost_usd: Math.round(cost),
      project_type: state.inputs.cocomo_project_type || 'organic'
    } : null;
  },

  truncate_files(state, config) {
    const maxFiles = state.inputs.max_files_per_language ?? 100;
    let didTruncate = false;

    for (const lang of state.languages) {
      if (lang.files_detail && lang.files_detail.length > maxFiles) {
        lang.files_detail = lang.files_detail.slice(0, maxFiles);
        didTruncate = true;
      }
    }

    if (didTruncate) {
      state.truncated = true;
    }
  },

  strip_null_fields(state, config) {
    if (state.languages) {
      for (const lang of state.languages) {
        if (lang.files_detail === null) delete lang.files_detail;
        if (lang.complexity === null) delete lang.complexity;
      }
    }
    if (state.output) {
      if (state.output.complexity_score === null) delete state.output.complexity_score;
      if (state.output.cocomo === null) delete state.output.cocomo;
    }
  },

  // ── Git Blame Steps ──────────────────────────────────────

  parse_git_blame(state, config) {
    const stdout = state.result.stdout || '';
    const lines = [];
    const showEmail = state.inputs.show_email ?? false;

    let cache = { hash: null, author: null, email: null, date: null, date_relative: null, summary: null };
    let currentLine = null;

    const rawLines = stdout.split('\n');

    for (let i = 0; i < rawLines.length; i++) {
      const rawLine = rawLines[i];

      if (rawLine.startsWith('\t')) {
        // Line text — finalize current line
        if (currentLine) {
          currentLine.line_text = rawLine.slice(1);
          currentLine.author = showEmail && cache.email
            ? cache.email.replace(/^<|>$/g, '')
            : (cache.author || '');
          currentLine.date = cache.date || '';
          currentLine.date_relative = cache.date_relative || '';
          currentLine.summary = cache.summary || '';
          lines.push(currentLine);
          currentLine = null;
        }
        continue;
      }

      // Check for hash line: 40-char hex followed by space and digits
      const hashMatch = rawLine.match(/^[0-9a-f]{40}(?=\s)/);
      if (hashMatch) {
        const parts = rawLine.split(' ');
        const hash = parts[0];
        const hasGroupSize = parts.length === 4;

        // Reset cache when we encounter a new group (hasGroupSize) or a different hash
        if (hasGroupSize || cache.hash !== hash) {
          cache = { hash, author: null, email: null, date: null, date_relative: null, summary: null };
        }

        currentLine = {
          line_number: parseInt(parts[2], 10),
          commit_hash: hash,
          short_commit: hash.slice(0, 7)
        };
        continue;
      }

      // Metadata fields
      if (rawLine.startsWith('author ')) {
        cache.author = rawLine.slice(7);
      } else if (rawLine.startsWith('author-mail ')) {
        cache.email = rawLine.slice(12);
      } else if (rawLine.startsWith('author-time ')) {
        const timestamp = parseInt(rawLine.slice(12), 10);
        if (!isNaN(timestamp)) {
          cache.date = new Date(timestamp * 1000).toISOString();
          cache.date_relative = formatRelativeDate(timestamp);
        }
      } else if (rawLine.startsWith('summary ')) {
        cache.summary = rawLine.slice(8);
      }
    }

    state.lines = lines;
  },

  truncate_blame_lines(state, config) {
    const limit = state.inputs.max_lines ?? state.spec.inputs.max_lines?.default ?? 500;
    if (state.lines.length > limit) {
      state.lines = state.lines.slice(0, limit);
      state.truncated = true;
    }
  },

  // ── Git Stash Steps ────────────────────────────────────

  parse_git_stash(state, config) {
    const action = state.inputs.action || 'list';
    const stdout = state.result.stdout || '';
    state.stashes = [];
    state.action = action;

    if (action === 'list') {
      const startMarker = '>>>STASH_START<<<';
      const endMarker = '>>>STASH_END<<<';
      let pos = 0;
      while (true) {
        const startIdx = stdout.indexOf(startMarker, pos);
        if (startIdx === -1) break;
        const endIdx = stdout.indexOf(endMarker, startIdx);
        if (endIdx === -1) break;
        const part = stdout.slice(startIdx + startMarker.length, endIdx).replace(/^\n+/, '');
        const lines = part.split('\n').map(l => l.trimEnd()).filter(l => l.length > 0);
        if (lines.length >= 2) {
          state.stashes.push({
            index: extractStashIndex(lines[0]),
            hash: lines[1] || '',
            message: lines[2] || '',
            date: lines[3] || '',
            date_relative: lines[4] || ''
          });
        }
        pos = endIdx + endMarker.length;
      }
      // Fallback: parse plaintext "stash@{N}: message" format
      if (state.stashes.length === 0) {
        const lines = stdout.split('\n').filter(l => l.trim().length > 0);
        for (const line of lines) {
          const match = line.match(/^(stash@\{\d+\}):\s*(.+)$/);
          if (match) {
            state.stashes.push({
              index: extractStashIndex(match[1]),
              message: match[2],
              hash: '',
              date: '',
              date_relative: ''
            });
          }
        }
      }
    } else if (action === 'show' && !state.inputs.include_patch) {
      const lines = stdout.split('\n').filter(l => l.trim().length > 0);
      const files = [];
      for (const line of lines) {
        const parts = line.trim().split(/\t+/);
        if (parts.length >= 3) {
          const additions = parseInt(parts[0], 10);
          const deletions = parseInt(parts[1], 10);
          const filePath = parts[2];
          if (!isNaN(additions) && !isNaN(deletions) && filePath) {
            files.push({ path: filePath, additions, deletions });
          }
        }
      }
      state.stashes = [{
        index: state.inputs.stash_index ?? 0,
        files
      }];
    }
    // For mutating actions (push, apply, pop, drop, clear, branch),
    // stashes stays empty and action flags are set in assemble_output.
  },

  truncate_stashes(state, config) {
    const limit = state.inputs.max_results ?? state.spec.inputs.max_results?.default ?? 50;
    if (state.stashes.length > limit) {
      state.stashes = state.stashes.slice(0, limit);
      state.truncated = true;
    }
  },

  // ── Git Log Steps ────────────────────────────────────────

  parse_git_log(state, config) {
    const stdout = state.result.stdout || '';
    const commits = [];

    const startMarker = '>>>COMMIT_START<<<';
    const metaMarker = '>>>META<<<';
    const endMarker = '>>>COMMIT_END<<<';

    let pos = 0;
    while (true) {
      const startIdx = stdout.indexOf(startMarker, pos);
      if (startIdx === -1) break;

      const metaIdx = stdout.indexOf(metaMarker, startIdx);
      if (metaIdx === -1) break;

      const endIdx = stdout.indexOf(endMarker, metaIdx);
      if (endIdx === -1) break;

      // Header: everything between START and META
      const headerPart = stdout.slice(startIdx + startMarker.length, metaIdx).replace(/^\n+/, '');
      const headerLines = headerPart.split('\n');
      const hash = headerLines[0] || '';
      const shortHash = headerLines[1] || '';
      const subject = headerLines[2] || '';
      const body = headerLines.slice(3).join('\n').trim();

      // Meta: everything between META and END
      const metaPart = stdout.slice(metaIdx + metaMarker.length, endIdx).replace(/^\n+/, '');
      const metaLines = metaPart.split('\n');
      const author = metaLines[0] || '';
      const committer = metaLines[1] || '';
      const date = metaLines[2] || '';
      const dateRelative = metaLines[3] || '';

      // Extra: everything between END and next START (or EOF)
      const nextStartIdx = stdout.indexOf(startMarker, endIdx + endMarker.length);
      const extraPart = nextStartIdx === -1
        ? stdout.slice(endIdx + endMarker.length)
        : stdout.slice(endIdx + endMarker.length, nextStartIdx);
      const extraLines = extraPart.split('\n').map(l => l.trimEnd()).filter(l => l.length > 0);

      let stats = null;
      const filesChanged = [];

      // Parse stats summary line: "X files changed, Y insertions(+), Z deletions(-)"
      const statsLine = extraLines.find(l => /files? changed/.test(l));
      if (statsLine) {
        const filesMatch = statsLine.match(/(\d+)\s+file/);
        const insertionsMatch = statsLine.match(/(\d+)\s+insertion/);
        const deletionsMatch = statsLine.match(/(\d+)\s+deletion/);
        stats = {
          files: filesMatch ? parseInt(filesMatch[1], 10) : 0,
          insertions: insertionsMatch ? parseInt(insertionsMatch[1], 10) : 0,
          deletions: deletionsMatch ? parseInt(deletionsMatch[1], 10) : 0
        };
      }

      // Parse file lines from --stat or --name-only
      for (const line of extraLines) {
        if (line.includes('|') && !line.includes('files changed') && !line.includes('insertion') && !line.includes('deletion')) {
          const fileName = line.split('|')[0].trim();
          if (fileName) filesChanged.push(fileName);
        } else if (!line.includes('|') && !line.includes('files changed') && !line.includes('insertion') && !line.includes('deletion')) {
          if (line.length > 0 && !line.startsWith('>>>') && !line.startsWith('-')) {
            filesChanged.push(line);
          }
        }
      }

      const commit = {
        hash,
        short_hash: shortHash,
        subject,
        body,
        author,
        committer,
        date,
        date_relative: dateRelative
      };

      if (stats) {
        commit.stats = stats;
      }
      if (filesChanged.length > 0) {
        commit.files_changed = [...new Set(filesChanged)];
      }

      commits.push(commit);
      pos = endIdx + endMarker.length;
    }

    state.commits = commits;
  },

  // ── Git Branch Steps ─────────────────────────────────────

  parse_git_branch(state, config) {
    const stdout = state.result.stdout || '';
    const lines = stdout.split('\n');
    const branches = [];
    let currentBranch = null;

    for (const line of lines) {
      const trimmed = line.trimEnd();
      if (!trimmed) continue;

      // Skip symbolic refs like "remotes/origin/HEAD -> origin/main"
      if (trimmed.includes(' -> ') && !/[a-f0-9]{7,40}/.test(trimmed)) {
        continue;
      }

      // Match branch line: marker char, space, branch name, hash, rest
      // Marker: * = current, + = checked out elsewhere, ' ' = not current
      const match = trimmed.match(/^([*+]) (.*?)\s+([a-f0-9]{7,40})\s+(.*)$/);
      const matchNonCurrent = trimmed.match(/^  (.*?)\s+([a-f0-9]{7,40})\s+(.*)$/);

      let isCurrent = false;
      let name, commit, rest;

      if (match) {
        isCurrent = match[1] === '*';
        name = match[2].trim();
        commit = match[3];
        rest = match[4];
      } else if (matchNonCurrent) {
        name = matchNonCurrent[1].trim();
        commit = matchNonCurrent[2];
        rest = matchNonCurrent[3];
      } else {
        continue;
      }

      // Parse upstream info from rest: [upstream: ahead X, behind Y] subject
      let remote = null;
      let ahead = 0;
      let behind = 0;
      let subject = rest;

      const upstreamMatch = rest.match(/^\[(.*?)\]\s*(.*)$/);
      if (upstreamMatch) {
        const upstreamInfo = upstreamMatch[1];
        subject = upstreamMatch[2];

        // Extract remote name (part before colon, or whole string)
        const colonIdx = upstreamInfo.indexOf(':');
        if (colonIdx !== -1) {
          remote = upstreamInfo.slice(0, colonIdx).trim();
          const details = upstreamInfo.slice(colonIdx + 1);

          const aheadMatch = details.match(/ahead\s+(\d+)/);
          if (aheadMatch) ahead = parseInt(aheadMatch[1], 10);

          const behindMatch = details.match(/behind\s+(\d+)/);
          if (behindMatch) behind = parseInt(behindMatch[1], 10);
        } else {
          remote = upstreamInfo.trim();
        }
      }

      const branch = {
        name,
        current: isCurrent,
        commit,
        subject: subject.trim()
      };

      if (remote) {
        branch.remote = remote;
      }
      if (ahead > 0) {
        branch.ahead = ahead;
      }
      if (behind > 0) {
        branch.behind = behind;
      }

      // Infer merged status from inputs
      if (state.inputs.merged) {
        branch.merged = true;
      } else if (state.inputs.no_merged) {
        branch.merged = false;
      }

      branches.push(branch);

      if (isCurrent) {
        currentBranch = name;
      }
    }

    state.branches = branches;
    state.current_branch = currentBranch;
  },

  truncate_branches(state, config) {
    const limit = state.inputs.max_results ?? state.spec.inputs.max_results?.default ?? 100;
    if (state.branches.length > limit) {
      state.branches = state.branches.slice(0, limit);
      state.truncated = true;
    }
  },

  // ── Find & Replace Steps ─────────────────────────────────

  validate_schema(state, config) {
    // Basic validation: ensure required fields exist
    if (!state.parsed) return;
    // If the wrapper already produces the correct schema, this is a no-op pass
  },

  truncate_changes(state, config) {
    const maxFiles = state.inputs.max_files || 1000;
    if (Array.isArray(state.parsed?.changes) && state.parsed.changes.length > maxFiles) {
      state.parsed.changes = state.parsed.changes.slice(0, maxFiles);
      state.parsed.truncated = true;
    }
  },

  strip_empty_replacements(state, config) {
    if (!Array.isArray(state.parsed?.changes)) return;

    for (const change of state.parsed.changes) {
      if (change.replacements_count === 0) {
        delete change.replacements;
      }
      if (!state.inputs.dry_run) {
        delete change.preview;
      }
    }
  },

  parse_grep_plaintext(state, config) {
    // Parse grep -rn output: "file:line:text"
    const lines = state.result.stdout.split('\n').filter(l => l.trim().length > 0);
    const matches = [];

    for (const line of lines) {
      // Format: path:line_number:match_text
      const firstColon = line.indexOf(':');
      if (firstColon === -1) continue;

      const file = line.slice(0, firstColon);
      const rest = line.slice(firstColon + 1);

      const secondColon = rest.indexOf(':');
      if (secondColon === -1) continue;

      const lineNum = parseInt(rest.slice(0, secondColon), 10);
      const text = rest.slice(secondColon + 1);

      if (file && !isNaN(lineNum)) {
        matches.push({
          file,
          line_number: lineNum,
          column_start: null,
          column_end: null,
          match_text: null,  // Grep doesn't isolate match text
          line_text: text
        });
      }
    }

    state.matches = matches;
  },

  compute_totals_fr(state, config) {
    // Find_replace specific totals
    const changes = state.parsed?.changes || [];
    state.total_files_processed = changes.length;
    state.total_replacements = changes.reduce((s, c) => s + (c.replacements_count || 0), 0);
    state.files_modified = changes.filter(c => c.modified).length;
    state.files_unchanged = changes.filter(c => !c.modified).length;
  }
};

// compute_totals_fr is merged into compute_totals above for skill-aware routing

// ── Helper Functions ──────────────────────────────────────

function resolvePath(obj, path) {
  if (!path) return undefined;
  const parts = path.split('.');
  let current = obj;
  for (const part of parts) {
    if (current == null) return undefined;
    // Handle array indexing like submatches[0]
    const arrMatch = part.match(/^(.+)\[(\d+)\]$/);
    if (arrMatch) {
      current = current[arrMatch[1]];
      if (Array.isArray(current)) {
        current = current[parseInt(arrMatch[2], 10)];
      }
    } else {
      current = current[part];
    }
  }
  return current;
}

function resolveSccField(lang, src) {
  // Map scc field names to our schema
  const mapping = {
    'Name': 'name',
    'Count': 'files',
    'Code': 'code',
    'Comment': 'comments',
    'Blank': 'blanks',
    'Complexity': 'complexity',
    'Bytes': 'bytes'
  };

  if (src === 'Name') return lang.Name;
  if (src === 'Count') return lang.Count;
  if (src === 'Code') return lang.Code;
  if (src === 'Comment') return lang.Comment;
  if (src === 'Blank') return lang.Blank;
  if (src === 'Complexity') return lang.Complexity;
  if (src === 'sum(Code, Comment, Blank)') {
    return (lang.Code || 0) + (lang.Comment || 0) + (lang.Blank || 0);
  }
  return lang[src];
}

function evaluateExpression(obj, expr) {
  // Simple expression evaluator for things like:
  // "data.elapsed_total.secs * 1000 + data.elapsed_total.nanos / 1000000"
  try {
    const secs = resolvePath(obj, 'data.elapsed_total.secs') || 0;
    const nanos = resolvePath(obj, 'data.elapsed_total.nanos') || 0;
    return secs * 1000 + nanos / 1000000;
  } catch {
    return 0;
  }
}

function sum(arr, field) {
  return arr.reduce((s, item) => s + (item[field] || 0), 0);
}

function extractStashIndex(ref) {
  const match = ref.match(/stash@\{(\d+)\}/);
  return match ? parseInt(match[1], 10) : 0;
}

function formatRelativeDate(timestamp) {
  const now = Date.now() / 1000;
  const diff = now - timestamp;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)} minutes ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} hours ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)} days ago`;
  if (diff < 2592000) return `${Math.floor(diff / 604800)} weeks ago`;
  if (diff < 31536000) return `${Math.floor(diff / 2592000)} months ago`;
  return `${Math.floor(diff / 31536000)} years ago`;
}

// Polyfill for structuredClone if not available
function structuredClone(obj) {
  return JSON.parse(JSON.stringify(obj));
}
