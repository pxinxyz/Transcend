/**
 * NunjucksEngine — Renders skill execution templates into CLI arguments.
 *
 * Every skill declares its CLI invocation as an array of Nunjucks template strings.
 * The engine takes the skill's args templates + user-provided inputs, and produces
 * a flat array of strings ready for child_process.spawn.
 *
 * Conditional flags ({% if x %}), loops ({% for %}), and variable interpolation
 * ({{ var }}) are all resolved against the merged input context (defaults + user values).
 */

import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const nunjucks = require('nunjucks');

// Configure nunjucks for CLI rendering:
// - No autoescaping (we're generating shell args, not HTML)
// - Trim blocks to avoid extra whitespace in output
// - Throw on undefined variables (strict mode for agent safety)
const NJ_ENV = new nunjucks.Environment(null, {
  autoescape: false,
  trimBlocks: true,
  lstripBlocks: true,
  throwOnUndefined: false  // We'll handle undefined ourselves for better errors
});

// Custom filter: render a value only if it's truthy, else return empty string
NJ_ENV.addFilter('defined', val => val !== undefined && val !== null);

export class NunjucksEngine {
  constructor(options = {}) {
    this.strict = options.strict ?? true;
    this.debug = options.debug ?? false;
    this.templateCache = new Map();
  }

  /**
   * Render CLI arguments from a skill spec and user inputs.
   *
   * @param {Object} spec — the loaded skill specification
   * @param {Object} userInputs — key/value pairs provided by the agent
   * @returns {String[]} — flat array of CLI argument strings
   */
  renderArgs(spec, userInputs = {}) {
    const context = this._buildContext(spec.inputs, userInputs);
    const templates = spec.execution.args;

    if (this.debug) {
      console.error('[NunjucksEngine] Rendering templates:', templates);
      console.error('[NunjucksEngine] Context:', JSON.stringify(context, null, 2));
    }

    const rendered = [];
    for (const template of templates) {
      try {
        let compiled = this.templateCache.get(template);
        if (!compiled) {
          compiled = nunjucks.compile(template, NJ_ENV);
          this.templateCache.set(template, compiled);
        }
        let result = compiled.render(context);

        // Fix: for-loop templates like "{% for ft in file_types %}--type={{ ft }}{% endfor %}"
        // render without spaces between iterations: "--type=rust--type=ts"
        // Detect and split concatenated CLI flags of the form --key=value--key=value
        result = this._splitConcatenatedFlags(result);

        // Split rendered string into arguments, respecting double-quoted segments
        // so that "export class" stays as one argument
        const parts = this._smartSplit(result);

        rendered.push(...parts);
      } catch (err) {
        throw new TemplateRenderError(
          `Failed to render template: "${template}"\n` +
          `Context: ${JSON.stringify(context)}\n` +
          `Error: ${err.message}`
        );
      }
    }

    if (this.debug) {
      console.error('[NunjucksEngine] Rendered args:', rendered);
    }

    return rendered;
  }

  /**
   * Render a single template string (used for fallback args, custom templates).
   */
  renderString(template, context) {
    try {
      return NJ_ENV.renderString(template, context);
    } catch (err) {
      throw new TemplateRenderError(
        `Failed to render template: "${template}"\n` +
        `Error: ${err.message}`
      );
    }
  }

  /**
   * Build the merged context: apply defaults from the spec, then override
   * with user-provided values. Validates required inputs.
   */
  _buildContext(inputSpecs, userInputs) {
    const context = {};

    for (const [name, def] of Object.entries(inputSpecs)) {
      // User-provided value takes precedence
      if (name in userInputs) {
        const value = userInputs[name];

        // Type validation
        this._validateType(name, value, def);
        context[name] = value;
        continue;
      }

      // Apply default if available
      if ('default' in def) {
        context[name] = def.default;
        continue;
      }

      // Required but not provided
      if (def.required) {
        throw new TemplateRenderError(
          `Missing required input: "${name}"`
        );
      }

      // Optional with no default -> undefined in context
      // Nunjucks will treat undefined as falsy in conditionals
      context[name] = undefined;
    }

    return context;
  }

  _validateType(name, value, def) {
    const expectedType = def.type;
    const actualType = Array.isArray(value) ? 'array' : typeof value;

    switch (expectedType) {
      case 'string':
        if (typeof value !== 'string') {
          throw new TemplateRenderError(
            `Input "${name}" must be a string, got ${actualType}`
          );
        }
        break;
      case 'integer':
        if (!Number.isInteger(value)) {
          throw new TemplateRenderError(
            `Input "${name}" must be an integer, got ${actualType} (${value})`
          );
        }
        break;
      case 'boolean':
        if (typeof value !== 'boolean') {
          throw new TemplateRenderError(
            `Input "${name}" must be a boolean, got ${actualType}`
          );
        }
        break;
      case 'array':
        if (!Array.isArray(value)) {
          throw new TemplateRenderError(
            `Input "${name}" must be an array, got ${actualType}`
          );
        }
        break;
      case 'number':
        if (typeof value !== 'number' || Number.isNaN(value)) {
          throw new TemplateRenderError(
            `Input "${name}" must be a number, got ${actualType}`
          );
        }
        break;
      default:
        // Unknown type — allow anything (for extensibility)
        break;
    }

    // Enum validation
    if (def.enum && !def.enum.includes(value)) {
      throw new TemplateRenderError(
        `Input "${name}" must be one of [${def.enum.join(', ')}], got "${value}"`
      );
    }

    // Range validation for integers
    if (expectedType === 'integer') {
      if ('minimum' in def && value < def.minimum) {
        throw new TemplateRenderError(
          `Input "${name}" must be >= ${def.minimum}, got ${value}`
        );
      }
      if ('maximum' in def && value > def.maximum) {
        throw new TemplateRenderError(
          `Input "${name}" must be <= ${def.maximum}, got ${value}`
        );
      }
    }

    // String length validation
    if (expectedType === 'string') {
      if ('min_length' in def && value.length < def.min_length) {
        throw new TemplateRenderError(
          `Input "${name}" must be at least ${def.min_length} characters, got ${value.length}`
        );
      }
      if ('max_length' in def && value.length > def.max_length) {
        throw new TemplateRenderError(
          `Input "${name}" must be at most ${def.max_length} characters, got ${value.length}`
        );
      }
    }

    // Array length validation
    if (expectedType === 'array') {
      if ('min_length' in def && value.length < def.min_length) {
        throw new TemplateRenderError(
          `Input "${name}" must have at least ${def.min_length} items, got ${value.length}`
        );
      }
      if ('max_length' in def && value.length > def.max_length) {
        throw new TemplateRenderError(
          `Input "${name}" must have at most ${def.max_length} items, got ${value.length}`
        );
      }
    }
  }

  /**
   * Pre-render validation: check if all required inputs are present
   * without actually rendering (for fast-fail before execution).
   */
  /**
   * Detect and split concatenated CLI flags like "--type=rust--type=ts"
   * into "--type=rust --type=ts". This handles for-loop templates that
   * don't include whitespace between iterations.
   */
  _splitConcatenatedFlags(str) {
    // Match patterns where a --flag follows immediately after another value
    // e.g., --type=rust--type=ts -> --type=rust --type=ts
    // e.g., --glob=!node_modules/--glob=!*.log -> --glob=!node_modules/ --glob=!*.log
    // Only split when -- follows non-whitespace (concatenated)
    return str.replace(/([^\s])(--[a-zA-Z0-9_-]+)/g, '$1 $2');
  }

  /**
   * Split a string on whitespace while respecting double-quoted segments.
   * "export class" --flag → ['export class', '--flag']
   */
  _smartSplit(str) {
    const parts = [];
    let current = '';
    let inQuotes = false;

    for (let i = 0; i < str.length; i++) {
      const ch = str[i];

      if (ch === '"') {
        inQuotes = !inQuotes;
        continue;  // Don't include the quote character
      }

      if (ch === ' ' && !inQuotes) {
        if (current.length > 0) {
          parts.push(current);
          current = '';
        }
        continue;
      }

      current += ch;
    }

    if (current.length > 0) {
      parts.push(current);
    }

    return parts;
  }

  validateInputs(spec, userInputs) {
    const errors = [];

    for (const [name, def] of Object.entries(spec.inputs)) {
      if (def.required && !(name in userInputs)) {
        errors.push(`Missing required input: "${name}"`);
        continue;
      }

      if (name in userInputs) {
        try {
          this._validateType(name, userInputs[name], def);
        } catch (err) {
          errors.push(err.message);
        }
      }
    }

    return errors;
  }
}

export class TemplateRenderError extends Error {
  constructor(message) {
    super(message);
    this.name = 'TemplateRenderError';
    this.code = 'TEMPLATE_RENDER_ERROR';
  }
}
