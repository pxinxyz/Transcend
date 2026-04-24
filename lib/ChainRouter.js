/**
 * ChainRouter — Routes typed data between skills without LLM context relay.
 *
 * When skills chain, data flows directly through the runtime. The upstream
 * skill declares what it produces (e.g., file_path, line_number); the downstream
 * skill declares what it accepts. The ChainRouter:
 *
 *   1. Validates compatibility between upstream output and downstream input
 *   2. Extracts values from the upstream envelope (field mapping)
 *   3. Aggregates scalar values into arrays when needed
 *   4. Builds the downstream input context
 *   5. Executes the downstream skill
 *   6. Returns the final result
 */

export class ChainRouter {
  constructor(runtime, options = {}) {
    this.runtime = runtime;
    this.debug = options.debug ?? false;
  }

  /**
   * Execute a single skill.
   */
  async execute(skillName, inputs) {
    return this.runtime.execute(skillName, inputs);
  }

  /**
   * Chain two skills: run upstream, then feed its output into downstream.
   *
   * @param {string} upstreamSkill — first skill to execute
   * @param {Object} upstreamInputs — inputs for the first skill
   * @param {string} downstreamSkill — second skill to execute
   * @param {Object} downstreamOverrides — additional inputs for downstream (beyond chained data)
   * @returns {Promise<Object>} — downstream skill result
   */
  async chain(upstreamSkill, upstreamInputs, downstreamSkill, downstreamOverrides = {}) {
    // Step 1: Run upstream skill
    const upstreamResult = await this.execute(upstreamSkill, upstreamInputs);

    if (upstreamResult.status === 'error') {
      // Upstream failed — return the error (or could throw)
      return upstreamResult;
    }

    // Step 2: Extract chainable values from upstream result
    const chainData = this._extractChainData(upstreamSkill, downstreamSkill, upstreamResult);

    if (this.debug) {
      console.error(`[ChainRouter] Chaining ${upstreamSkill} → ${downstreamSkill}`);
      console.error(`[ChainRouter] Extracted:`, JSON.stringify(chainData, null, 2));
    }

    // Step 3: Merge with downstream overrides
    const downstreamInputs = {
      ...chainData,
      ...downstreamOverrides
    };

    // Step 4: Run downstream skill
    return this.execute(downstreamSkill, downstreamInputs);
  }

  /**
   * Multi-skill pipeline: execute a sequence of skills, feeding each output
   * into the next. Returns the final result plus all intermediate results.
   *
   * @param {Array<{skill: string, inputs: Object, extract: string[]}>} steps
   * @returns {Promise<{results: Object[], final: Object}>}
   */
  async pipeline(steps) {
    const results = [];
    let lastResult = null;

    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      let inputs = { ...step.inputs };

      // If there's a previous result, extract chainable data
      if (lastResult && lastResult.status === 'ok') {
        const prevSkill = steps[i - 1].skill;
        const chainData = this._extractChainData(prevSkill, step.skill, lastResult);
        inputs = { ...chainData, ...inputs };
      }

      const result = await this.execute(step.skill, inputs);
      results.push({
        skill: step.skill,
        index: i,
        result
      });

      lastResult = result;

      // Stop pipeline on error
      if (result.status === 'error') {
        break;
      }
    }

    return {
      results,
      final: lastResult
    };
  }

  /**
   * Check if two skills are chainable.
   */
  isChainable(upstreamSkill, downstreamSkill) {
    const spec = this.runtime.loader.load(upstreamSkill);
    if (!spec.chains?.compatible_downstream) return false;

    return spec.chains.compatible_downstream.some(
      c => c.skill === downstreamSkill
    );
  }

  /**
   * Get all downstream skills compatible with a given skill.
   */
  getDownstreamSkills(skillName) {
    try {
      const spec = this.runtime.loader.load(skillName);
      return spec.chains?.compatible_downstream?.map(c => c.skill) || [];
    } catch {
      return [];
    }
  }

  /**
   * Extract chainable data from an upstream result envelope.
   *
   * Handles:
   *   - Scalar → scalar direct pass
   *   - Scalar → array automatic aggregation (deduplicated)
   *   - Nested field extraction (e.g., "matches.file")
   *   - Multiple field extraction
   */
  _extractChainData(upstreamSkill, downstreamSkill, envelope) {
    const spec = this.runtime.loader.load(upstreamSkill);
    const chainDef = spec.chains?.compatible_downstream?.find(
      c => c.skill === downstreamSkill
    );

    if (!chainDef) {
      // No explicit chain declaration — try generic field mapping
      return this._genericExtract(envelope);
    }

    const extracted = {};
    for (const via of chainDef.via) {
      const values = this._pluckField(envelope, via);
      if (values !== undefined) {
        // Merge into extracted — if field already exists, merge arrays
        if (Array.isArray(values)) {
          extracted[via] = extracted[via]
            ? [...new Set([...extracted[via], ...values])]
            : values;
        } else {
          extracted[via] = values;
        }
      }
    }

    return extracted;
  }

  /**
   * Generic extraction: pull common fields from any envelope.
   */
  _genericExtract(envelope) {
    const extracted = {};

    // Common chainable fields across skills
    if (envelope.matches) {
      extracted.files = [...new Set(envelope.matches.map(m => m.file))];
      extracted.pattern = envelope.matches[0]?.match_text;
    }

    if (envelope.changes) {
      extracted.files = [...new Set(envelope.changes.map(c => c.file))];
    }

    if (envelope.languages) {
      extracted.languages = envelope.languages.map(l => l.name);
      if (envelope.languages[0]?.files_detail) {
        extracted.files = envelope.languages.flatMap(l =>
          l.files_detail.map(f => f.path)
        );
      }
    }

    return extracted;
  }

  /**
   * Pluck a potentially nested field from an envelope.
   * Handles array traversal (e.g., "matches.file" → array of file paths).
   */
  _pluckField(obj, path) {
    const parts = path.split('.');

    // If the path has one part, direct access
    if (parts.length === 1) {
      return obj[path];
    }

    // If first part is an array, pluck from each element
    const first = parts[0];
    const rest = parts.slice(1);

    if (Array.isArray(obj[first])) {
      const values = [];
      for (const item of obj[first]) {
        const val = this._getPath(item, rest);
        if (val !== undefined) values.push(val);
      }
      return values.length > 0 ? values : undefined;
    }

    // Otherwise, nested object access
    return this._getPath(obj[first], rest);
  }

  _getPath(obj, parts) {
    let current = obj;
    for (const part of parts) {
      if (current == null) return undefined;
      current = current[part];
    }
    return current;
  }
}
