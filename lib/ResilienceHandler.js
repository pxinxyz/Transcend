/**
 * ResilienceHandler — Retries, fallbacks, and error recovery.
 *
 * When a primary CLI tool fails (command not found, timeout, transient error),
 * the resilience system:
 *   1. Retries on specified error conditions (with exponential backoff)
 *   2. Falls back to an alternative tool if retries are exhausted
 *   3. Normalizes fallback output to the same schema as the primary
 *   4. Reports output gaps so downstream skills know what degraded
 *
 * This ensures skills work everywhere — even when the optimal tool isn't installed.
 */

import { NunjucksEngine } from './NunjucksEngine.js';

export class ResilienceHandler {
  constructor(options = {}) {
    this.nunjucks = new NunjucksEngine({ debug: options.debug });
    this.debug = options.debug ?? false;
  }

  /**
   * Check if the primary command for a skill is available.
   */
  async checkPrimary(spec, executor) {
    const cmd = spec.execution.command;
    const available = await executor.checkCommand(cmd);
    if (this.debug) {
      console.error(`[ResilienceHandler] Primary command "${cmd}": ${available ? 'available' : 'NOT FOUND'}`);
    }
    return available;
  }

  /**
   * Determine if we should retry based on the execution result and retry config.
   */
  shouldRetry(result, retryConfig, attemptNumber) {
    if (!retryConfig) return false;
    if (attemptNumber > (retryConfig.max_retries || 0)) return false;

    // Check exit codes
    if (retryConfig.on_exit_codes?.includes(result.exitCode)) {
      return true;
    }

    // Check error types (from error.code or exit code map)
    if (retryConfig.on_errors) {
      const errKey = result.exitStatus;
      if (retryConfig.on_errors.includes(errKey)) return true;

      // Check for ETIMEDOUT, EAGAIN, etc.
      if (result.timedOut && retryConfig.on_errors.includes('ETIMEDOUT')) {
        return true;
      }
    }

    return false;
  }

  /**
   * Compute backoff delay in milliseconds.
   */
  getBackoffDelay(retryConfig, attemptNumber) {
    const base = retryConfig.backoff_ms || 500;
    // Exponential backoff: base * 2^(attempt-1)
    return base * Math.pow(2, attemptNumber - 1);
  }

  /**
   * Check if we should activate the fallback based on execution result.
   */
  shouldFallback(result, fallbackConfig) {
    if (!fallbackConfig) return false;

    const condition = fallbackConfig.condition || '';

    if (condition === 'command_not_found' && result.commandNotFound) {
      return true;
    }
    if (condition === 'command_not_found' && result.exitStatus === 'command_not_found') {
      return true;
    }
    if (condition === 'nonzero_exit' && result.exitCode !== 0) {
      return true;
    }
    if (condition === 'timeout' && result.timedOut) {
      return true;
    }

    return false;
  }

  /**
   * Render fallback CLI arguments from the fallback configuration.
   * Uses the same input context as the primary.
   */
  renderFallbackArgs(spec, userInputs) {
    const fallback = spec.resilience?.fallback;
    if (!fallback) return null;

    if (!fallback.args) {
      // Fallback with no args template — just run the command directly
      return { command: fallback.command, args: [] };
    }

    const context = this._buildFallbackContext(spec.inputs, userInputs);

    // Use same rendering logic as primary: split concatenated flags, then smart split
    const args = [];
    for (const template of fallback.args) {
      let rendered = this.nunjucks.renderString(template, context);
      // Split concatenated flags: --type=rust--type=ts → --type=rust --type=ts
      rendered = this._splitConcatenatedFlags(rendered);

      // For variable-interpolation templates ({{ pattern }}, {{ path }}),
      // keep the value as a single arg even if it contains spaces
      const isInterpolation = /^\{\{\s*\w+\s*\}\}$/.test(template.trim());
      if (isInterpolation && rendered.includes(' ')) {
        if (rendered.trim().length > 0) {
          args.push(rendered.trim());
        }
        continue;
      }

      // Split on whitespace, respecting quoted segments
      const parts = this._smartSplit(rendered);
      args.push(...parts);
    }

    if (this.debug) {
      console.error(`[ResilienceHandler] Fallback args: ${fallback.command} ${args.join(' ')}`);
    }

    return { command: fallback.command, args };
  }

  /**
   * Detect and split concatenated CLI flags like "--type=rust--type=ts"
   * into "--type=rust --type=ts". Mirrors NunjucksEngine._splitConcatenatedFlags.
   */
  _splitConcatenatedFlags(str) {
    return str.replace(/([^\s])(--[a-zA-Z0-9_-]+)/g, '$1 $2');
  }

  /**
   * Split a string on whitespace while respecting double-quoted segments.
   * Mirrors NunjucksEngine._smartSplit.
   */
  _smartSplit(str) {
    const parts = [];
    let current = '';
    let inQuotes = false;

    for (let i = 0; i < str.length; i++) {
      const ch = str[i];
      if (ch === '"') { inQuotes = !inQuotes; continue; }
      if (ch === ' ' && !inQuotes) {
        if (current.length > 0) { parts.push(current); current = ''; }
        continue;
      }
      current += ch;
    }
    if (current.length > 0) parts.push(current);
    return parts;
  }

  /**
   * Build input context for fallback rendering. Same logic as primary.
   */
  _buildFallbackContext(inputSpecs, userInputs) {
    const context = {};
    for (const [name, def] of Object.entries(inputSpecs)) {
      if (name in userInputs) {
        context[name] = userInputs[name];
      } else if ('default' in def) {
        context[name] = def.default;
      } else {
        context[name] = undefined;
      }
    }
    return context;
  }

  /**
   * Mark an envelope as having used the fallback, and annotate output gaps.
   */
  annotateFallback(envelope, spec) {
    const fallback = spec.resilience?.fallback;
    if (!fallback) return envelope;

    const output = { ...envelope, used_fallback: true };

    // Add output gaps to warnings if present
    if (fallback.output_gaps?.length) {
      output.warnings = [
        ...(output.warnings || []),
        `Fallback mode (${fallback.command}): Some features are unavailable.`,
        ...fallback.output_gaps.map(g => `  - ${g}`)
      ];
    }

    // Apply output gap field nullifications
    for (const gap of fallback.output_gaps || []) {
      if (gap.includes('column_start and column_end will be null')) {
        for (const m of output.matches || []) {
          m.column_start = null;
          m.column_end = null;
        }
      }
      if (gap.includes('match_text isolation is unavailable')) {
        for (const m of output.matches || []) {
          m.match_text = null;
        }
      }
      if (gap.includes('files_searched count unavailable')) {
        output.files_searched = output.matches?.length || 0;
      }
      if (gap.includes('execution_ms unavailable')) {
        delete output.execution_ms;
      }
      if (gap.includes('truncated detection unavailable')) {
        output.truncated = false;
      }
      if (gap.includes('complexity is unavailable')) {
        output.complexity_score = null;
        for (const l of output.languages || []) {
          l.complexity = null;
        }
      }
      if (gap.includes('cocomo is unavailable')) {
        output.cocomo = null;
      }
      if (gap.includes('per-file granularity is unavailable')) {
        for (const l of output.languages || []) {
          delete l.files_detail;
        }
      }
    }

    return output;
  }

  /**
   * Apply fallback-specific normalization pipeline.
   * If the fallback declares a custom normalization name, we attempt to use it.
   */
  async runFallbackNormalization(spec, result, inputs, normalizePipeline) {
    const fallback = spec.resilience?.fallback;
    const normalizationName = fallback?.normalization;

    if (this.debug) {
      console.error(`[ResilienceHandler] Running fallback normalization: ${normalizationName || 'default'}`);
    }

    // Build a synthetic spec with a pipeline adapted for the fallback output format
    const adaptedSpec = this._buildFallbackSpec(spec, normalizationName);

    return normalizePipeline.run(adaptedSpec, result, inputs);
  }

  /**
   * Build a spec with a normalization pipeline adapted for fallback output.
   */
  _buildFallbackSpec(spec, normalizationName) {
    const adapted = { ...spec };
    adapted.normalization = { ...spec.normalization };

    // For grep plaintext fallback, inject plaintext parser before standard steps
    if (normalizationName === 'grep_plaintext_to_transcend_schema') {
      const basePipeline = spec.normalization?.pipeline || [];
      // Replace filter_message_types + extract_matches with parse_grep_plaintext
      const filtered = basePipeline.filter(s =>
        s.step !== 'filter_message_types' &&
        s.step !== 'extract_matches' &&
        s.step !== 'attach_context' &&
        s.step !== 'extract_summary'
      );
      adapted.normalization.pipeline = [
        { step: 'parse_grep_plaintext' },
        // Keep handle_no_matches, truncate, detect, assemble if present
        ...filtered
      ];
    }

    // For sed fallback, similar plaintext handling
    if (normalizationName === 'sed_silent_to_transcend_schema') {
      adapted.normalization.pipeline = [
        { step: 'parse_grep_plaintext' },  // sed output is similar format
        ...(spec.normalization?.pipeline || [])
      ];
    }

    // For tokei JSON fallback, use standard pipeline but add parse_json first
    if (normalizationName === 'tokei_json_to_transcend_schema') {
      const basePipeline = spec.normalization?.pipeline || [];
      // tokei outputs JSON but with different schema — would need custom mapping
      // For now, pass through standard pipeline
      adapted.normalization.pipeline = basePipeline;
    }

    // For git_config fallback, replace parse_git_config with git_config_short
    if (normalizationName === 'git_config_short') {
      const basePipeline = spec.normalization?.pipeline || [];
      adapted.normalization.pipeline = basePipeline.map(s =>
        s.step === 'parse_git_config' ? { step: 'git_config_short', description: 'Simplified git config parsing without show-origin/show-scope metadata.' } : s
      );
    }

    // For git_remote fallback, replace pipeline with git_remote_short
    if (normalizationName === 'git_remote_short') {
      adapted.normalization.pipeline = [
        { step: 'git_remote_short', description: 'Simplified git remote parsing that returns only remote names.' },
        { step: 'assemble_output' }
      ];
    }

    // For git_tag fallback, replace parse_git_tag with git_tag_short
    if (normalizationName === 'git_tag_short') {
      const basePipeline = spec.normalization?.pipeline || [];
      const filtered = basePipeline.filter(s => s.step !== 'parse_git_tag');
      adapted.normalization.pipeline = [
        { step: 'git_tag_short', description: 'Simplified git tag parsing that returns only tag names.' },
        ...filtered
      ];
    }

    return adapted;
  }
}
