/**
 * SkillLoader — Loads and validates .skill.json specification files.
 *
 * A skill spec is the single source of truth: inputs, outputs, execution templates,
 * normalization pipeline, resilience config, and chain declarations. The loader
 * validates structural integrity at load time so the runtime never encounters
 * malformed specs at execution time.
 */

import { readFileSync, accessSync, readdirSync } from 'fs';
import { resolve, extname, join } from 'path';

// Minimal schema validation — ensures all required sections are present
// and types are correct. Full JSON Schema validation can be layered on top.
const REQUIRED_ROOT_FIELDS = [
  'name', 'version', 'stability', 'description',
  'inputs', 'outputs', 'execution'
];

const REQUIRED_EXECUTION_FIELDS = [
  'engine', 'template_engine', 'command', 'args'
];

const VALID_STABILITY = ['stable', 'beta', 'experimental', 'deprecated'];

export class SkillLoader {
  constructor(options = {}) {
    this.skillsDir = options.skillsDir || resolve(process.cwd(), 'skills');
    this.cache = new Map();
    this.schemas = new Map();
  }

  /**
   * Load a single skill by name (e.g., "universal_search").
   * Looks for {skillsDir}/{name}.skill.json
   */
  load(name) {
    if (this.cache.has(name)) {
      return this.cache.get(name);
    }

    const path = resolve(this.skillsDir, `${name}.skill.json`);
    const spec = this._loadFromPath(path);

    this._validate(spec, name);
    this.cache.set(name, spec);
    return spec;
  }

  /**
   * Load all .skill.json files from the skills directory.
   */
  loadAll() {
    const entries = readdirSync(this.skillsDir);
    const specs = [];
    for (const entry of entries) {
      if (extname(entry) === '.json' && entry.includes('.skill.')) {
        const name = entry.replace('.skill.json', '');
        specs.push(this.load(name));
      }
    }
    return specs;
  }

  /**
   * Reload a skill (clear cache and reload).
   */
  reload(name) {
    this.cache.delete(name);
    return this.load(name);
  }

  /**
   * Clear all cached skills.
   */
  clear() {
    this.cache.clear();
  }

  _loadFromPath(path) {
    try {
      accessSync(path);
    } catch {
      throw new SkillLoadError(`Skill spec not found: ${path}`);
    }

    let raw;
    try {
      raw = readFileSync(path, 'utf-8');
    } catch (err) {
      throw new SkillLoadError(`Failed to read skill spec at ${path}: ${err.message}`);
    }

    let spec;
    try {
      spec = JSON.parse(raw);
    } catch (err) {
      throw new SkillLoadError(`Invalid JSON in skill spec ${path}: ${err.message}`);
    }

    return spec;
  }

  _validate(spec, name) {
    // Check all required root fields
    for (const field of REQUIRED_ROOT_FIELDS) {
      if (!(field in spec)) {
        throw new SkillValidationError(
          `Skill "${name}" missing required field: ${field}`
        );
      }
    }

    // Validate stability enum
    if (!VALID_STABILITY.includes(spec.stability)) {
      throw new SkillValidationError(
        `Skill "${name}" has invalid stability "${spec.stability}". ` +
        `Must be one of: ${VALID_STABILITY.join(', ')}`
      );
    }

    // Validate execution section
    const exec = spec.execution;
    for (const field of REQUIRED_EXECUTION_FIELDS) {
      if (!(field in exec)) {
        throw new SkillValidationError(
          `Skill "${name}".execution missing required field: ${field}`
        );
      }
    }

    // Validate that template_engine is nunjucks (the only engine we support)
    if (exec.template_engine !== 'nunjucks') {
      throw new SkillValidationError(
        `Skill "${name}" uses unsupported template engine "${exec.template_engine}". ` +
        `Only "nunjucks" is supported.`
      );
    }

    // Validate args is an array of strings
    if (!Array.isArray(exec.args) || !exec.args.every(a => typeof a === 'string')) {
      throw new SkillValidationError(
        `Skill "${name}".execution.args must be an array of strings`
      );
    }

    // Validate inputs have types
    for (const [inputName, inputDef] of Object.entries(spec.inputs)) {
      if (!inputDef.type) {
        throw new SkillValidationError(
          `Skill "${name}".inputs.${inputName} missing "type"`
        );
      }
    }

    // Validate outputs have success and error schemas
    if (!spec.outputs.success || !spec.outputs.error) {
      throw new SkillValidationError(
        `Skill "${name}".outputs must define both "success" and "error" schemas`
      );
    }

    // Validate normalization pipeline if present
    if (spec.normalization) {
      if (!spec.normalization.pipeline) {
        throw new SkillValidationError(
          `Skill "${name}".normalization missing "pipeline"`
        );
      }
      if (!Array.isArray(spec.normalization.pipeline)) {
        throw new SkillValidationError(
          `Skill "${name}".normalization.pipeline must be an array`
        );
      }
    }

    // Validate resilience config if present
    if (spec.resilience?.fallback) {
      const fb = spec.resilience.fallback;
      if (!fb.command) {
        throw new SkillValidationError(
          `Skill "${name}".resilience.fallback missing "command"`
        );
      }
    }

    // Validate chain declarations if present
    if (spec.chains?.compatible_downstream) {
      for (const chain of spec.chains.compatible_downstream) {
        if (!chain.skill || !chain.via) {
          throw new SkillValidationError(
            `Skill "${name}" chain declaration missing "skill" or "via"`
          );
        }
      }
    }
  }

  /**
   * Get chain compatibility info for a skill.
   */
  getChainInfo(name) {
    const spec = this.load(name);
    return spec.chains || null;
  }

  /**
   * Resolve the executable command path, considering platform.
   */
  resolveCommand(name) {
    const spec = this.load(name);
    const cmd = spec.execution.command;

    // Platform-specific extensions
    if (process.platform === 'win32' && !cmd.endsWith('.exe')) {
      return cmd + '.exe';
    }
    return cmd;
  }
}

export class SkillLoadError extends Error {
  constructor(message) {
    super(message);
    this.name = 'SkillLoadError';
    this.code = 'SKILL_LOAD_ERROR';
  }
}

export class SkillValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'SkillValidationError';
    this.code = 'SKILL_VALIDATION_ERROR';
  }
}
