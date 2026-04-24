/**
 * TranscendRuntime — The central orchestrator for skill execution.
 *
 * Coordinates: SkillLoader → NunjucksEngine → CommandExecutor →
 *              NormalizationPipeline → ResilienceHandler
 *
 * Usage:
 *   const runtime = new TranscendRuntime({ skillsDir: './skills' });
 *   const result = await runtime.execute('universal_search', {
 *     pattern: 'fetchUserData',
 *     path: './src',
 *     file_types: ['ts']
 *   });
 */

import { SkillLoader, SkillLoadError, SkillValidationError } from './SkillLoader.js';
import { NunjucksEngine, TemplateRenderError } from './NunjucksEngine.js';
import { CommandExecutor, ExecutionError } from './CommandExecutor.js';
import { NormalizationPipeline } from './NormalizationPipeline.js';
import { ResilienceHandler } from './ResilienceHandler.js';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const defaultSkillsDir = path.join(__dirname, '..', 'skills');

export class TranscendRuntime {
  constructor(options = {}) {
    this.skillsDir = options.skillsDir || defaultSkillsDir;
    this.debug = options.debug ?? false;
    this.timeoutMs = options.timeoutMs || 60000;

    // Sub-systems
    this.loader = new SkillLoader({ skillsDir: this.skillsDir });
    this.engine = new NunjucksEngine({ debug: this.debug });
    this.executor = new CommandExecutor({ timeoutMs: this.timeoutMs, debug: this.debug });
    this.normalizer = new NormalizationPipeline({ debug: this.debug });
    this.resilience = new ResilienceHandler({ debug: this.debug });
  }

  /**
   * Execute a skill by name with the given inputs.
   *
   * This is the main entry point. It:
   *   1. Loads the skill spec
   *   2. Validates inputs
   *   3. Renders Nunjucks templates into CLI args
   *   4. Executes the command
   *   5. Runs the normalization pipeline
   *   6. Handles retries and fallbacks
   *   7. Returns the structured JSON envelope
   *
   * @param {string} skillName — e.g. "universal_search"
   * @param {Object} inputs — key/value inputs for the skill
   * @returns {Promise<Object>} — structured result envelope
   */
  async execute(skillName, inputs = {}) {
    const startTime = Date.now();

    if (this.debug) {
      console.error(`[TranscendRuntime] Executing: ${skillName}`);
      console.error(`[TranscendRuntime] Inputs:`, JSON.stringify(inputs, null, 2));
    }

    // ── Step 1: Load skill specification ───────────────────
    let spec;
    try {
      spec = this.loader.load(skillName);
    } catch (err) {
      return {
        status: 'error',
        error_type: 'skill_not_found',
        message: err.message,
        exit_code: null,
        stderr: '',
        used_fallback: false
      };
    }

    // ── Step 2: Validate inputs ────────────────────────────
    const validationErrors = this.engine.validateInputs(spec, inputs);
    if (validationErrors.length > 0) {
      return {
        status: 'error',
        error_type: 'invalid_argument',
        message: validationErrors.join('; '),
        exit_code: null,
        stderr: '',
        used_fallback: false
      };
    }

    // ── Step 3: Check if primary command is available ──────
    const primaryAvailable = await this.resilience.checkPrimary(spec, this.executor);
    let usedFallback = false;

    // ── Step 4: Execute (with retries and fallback) ────────
    let result;
    let executionSpec = spec;
    let executionInputs = { ...inputs };

    if (primaryAvailable) {
      // Try primary command
      result = await this._executeWithRetries(spec, inputs);

      // If primary failed with a fallback-eligible error, try fallback
      if (this.resilience.shouldFallback(result, spec.resilience?.fallback)) {
        const fallbackResult = await this._executeFallback(spec, inputs);
        if (fallbackResult) {
          result = fallbackResult;
          usedFallback = true;
          executionSpec = this._buildFallbackSpec(spec);
        }
      }
    } else {
      // Primary not available — go straight to fallback
      const fallbackResult = await this._executeFallback(spec, inputs);
      if (fallbackResult) {
        result = fallbackResult;
        usedFallback = true;
        executionSpec = this._buildFallbackSpec(spec);
      } else {
        // Neither primary nor fallback available
        return {
          status: 'error',
          error_type: 'command_not_found',
          message: `Primary command "${spec.execution.command}" not found, ` +
                   `and fallback "${spec.resilience?.fallback?.command}" ` +
                   `is also unavailable.`,
          exit_code: null,
          stderr: '',
          used_fallback: false
        };
      }
    }

    // ── Step 5: Normalize output ───────────────────────────
    let envelope;
    try {
      if (usedFallback) {
        envelope = await this.resilience.runFallbackNormalization(
          executionSpec, result, executionInputs, this.normalizer
        );
        envelope = this.resilience.annotateFallback(envelope, spec);
      } else {
        envelope = this.normalizer.run(executionSpec, result, executionInputs);
      }
    } catch (err) {
      return {
        status: 'error',
        error_type: 'normalization_failed',
        message: `Output normalization failed: ${err.message}`,
        exit_code: result.exitCode,
        stderr: result.stderr,
        used_fallback: usedFallback
      };
    }

    // ── Step 6: Post-process envelope ──────────────────────
    if (envelope.status === 'ok') {
      envelope.execution_ms = envelope.execution_ms || (Date.now() - startTime);
      if (usedFallback) {
        envelope.used_fallback = true;
      }
    }

    if (this.debug) {
      console.error(`[TranscendRuntime] Result status: ${envelope.status}`);
      console.error(`[TranscendRuntime] Execution time: ${envelope.execution_ms || Date.now() - startTime}ms`);
    }

    return envelope;
  }

  /**
   * Execute with retry logic.
   */
  async _executeWithRetries(spec, inputs) {
    const retryConfig = spec.resilience?.retry;
    let lastResult = null;
    let attempt = 0;

    do {
      attempt++;

      // Render args from templates
      const args = this.engine.renderArgs(spec, inputs);

      // Handle stdin piping for skills that need it
      let stdinData = null;
      if (spec.execution.note?.includes('stdin') ||
          spec.execution.note?.includes('ARG_MAX') ||
          spec.execution.note?.includes('files')) {
        // If inputs.files is a large array, pipe it via stdin
        if (Array.isArray(inputs.files) && inputs.files.length > 0) {
          stdinData = JSON.stringify({ files: inputs.files });
        }
      }

      const result = await this.executor.execute({
        command: spec.execution.command,
        args,
        timeoutMs: spec.execution.timeout_ms || this.timeoutMs,
        stdinData,
        exitCodeMap: spec.execution.exit_code_map,
        cwd: inputs.cwd
      });

      lastResult = result;

      // Should we retry?
      if (this.resilience.shouldRetry(result, retryConfig, attempt)) {
        const delay = this.resilience.getBackoffDelay(retryConfig, attempt);
        if (this.debug) {
          console.error(`[TranscendRuntime] Retrying after ${delay}ms (attempt ${attempt})`);
        }
        await sleep(delay);
        continue;
      }

      // No retry needed
      break;

    } while (attempt < (retryConfig?.max_retries || 0) + 1);

    return lastResult;
  }

  /**
   * Execute the fallback command.
   */
  async _executeFallback(spec, inputs) {
    const fallback = spec.resilience?.fallback;
    if (!fallback) return null;

    if (this.debug) {
      console.error(`[TranscendRuntime] Attempting fallback: ${fallback.command}`);
    }

    // Check if fallback command exists
    const fallbackAvailable = await this.executor.checkCommand(fallback.command);
    if (!fallbackAvailable) {
      if (this.debug) {
        console.error(`[TranscendRuntime] Fallback command "${fallback.command}" not found`);
      }
      return null;
    }

    // Render fallback args
    const fallbackCmd = this.resilience.renderFallbackArgs(spec, inputs);
    if (!fallbackCmd) return null;

    // Handle special case: find_replace fallback with files array
    let stdinData = null;
    if (spec.name === 'find_replace' && Array.isArray(inputs.files)) {
      stdinData = JSON.stringify({ files: inputs.files });
    }

    return this.executor.execute({
      command: fallbackCmd.command,
      args: fallbackCmd.args,
      timeoutMs: spec.execution.timeout_ms || this.timeoutMs,
      stdinData,
      exitCodeMap: { '0': 'success', '1': 'error' },
      cwd: inputs.cwd
    });
  }

  /**
   * Build a synthetic spec for fallback execution so the normalizer
   * can process fallback output with the same pipeline.
   */
  _buildFallbackSpec(originalSpec) {
    const fallback = originalSpec.resilience?.fallback;
    return {
      ...originalSpec,
      execution: {
        ...originalSpec.execution,
        command: fallback.command,
        args: fallback.args || []
      },
      normalization: {
        ...originalSpec.normalization,
        pipeline: this._adaptFallbackPipeline(originalSpec)
      }
    };
  }

  /**
   * Adapt the normalization pipeline for fallback output.
   * Some steps may need to be skipped or replaced for fallback tools.
   */
  _adaptFallbackPipeline(spec) {
    const pipeline = spec.normalization?.pipeline || [];
    const fallback = spec.resilience?.fallback;
    const outputGaps = fallback?.output_gaps || [];

    // Start with the original pipeline
    const adapted = [...pipeline];

    // If fallback has no column info, skip column-related steps
    const hasNoColumns = outputGaps.some(g =>
      g.includes('column_start') || g.includes('column_end')
    );

    if (hasNoColumns) {
      // Remove extract_matches step (fallback produces different format)
      // The fallback normalizer will handle it
    }

    return adapted;
  }

  /**
   * Get metadata about available skills.
   */
  listSkills() {
    try {
      return this.loader.loadAll().map(s => ({
        name: s.name,
        version: s.version,
        stability: s.stability,
        category: s.category,
        description: s.description
      }));
    } catch {
      return [];
    }
  }

  /**
   * Get full spec for a skill.
   */
  getSpec(skillName) {
    return this.loader.load(skillName);
  }
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// ── Convenience factory ─────────────────────────────────────

export function createRuntime(options) {
  return new TranscendRuntime(options);
}
