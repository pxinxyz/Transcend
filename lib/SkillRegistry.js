/**
 * SkillRegistry — Discovery, filtering, introspection, and health monitoring
 * for the Transcend skill framework.
 */

import { SkillLoader, SkillValidationError } from './SkillLoader.js';
import { spawn } from 'child_process';

const STABILITY_LEVELS = {
  stable: 4,
  beta: 3,
  experimental: 2,
  deprecated: 1
};

const INSTALL_COMMANDS = {
  rg: 'cargo install ripgrep  # or use system package manager',
  delta: 'cargo install git-delta  # or use system package manager',
  jq: 'brew install jq  # or apt-get install jq',
  scc: 'go install github.com/boyter/scc/v3@latest',
  git: 'install git via your system package manager',
  node: 'install Node.js >= 18 from https://nodejs.org',
  python3: 'install Python 3 via your system package manager',
  grep: 'install grep via your system package manager',
  bat: 'cargo install bat  # or use system package manager',
  fd: 'cargo install fd-find  # or use system package manager',
  eza: 'cargo install eza  # or use system package manager',
  sd: 'cargo install sd  # or use system package manager',
  ast_grep: 'npm install -g @ast-grep/cli',
  fzf: 'install fzf via your system package manager',
  delta_exe: 'cargo install git-delta  # or use system package manager',
  rg_exe: 'cargo install ripgrep  # or use system package manager'
};

export class SkillRegistry {
  constructor(options = {}) {
    this.loader = options.loader || new SkillLoader(options);
    this._skills = null;
  }

  /**
   * Discover and load all skills from the skills directory.
   */
  discover() {
    this._skills = this.loader.loadAll();
    return this._skills;
  }

  /**
   * Get all discovered skills. Lazily calls discover() if needed.
   */
  getAll() {
    if (!this._skills) {
      this.discover();
    }
    return this._skills;
  }

  /**
   * Filter skills by category, stability, or minimum stability level.
   */
  filter({ category, stability, minStability } = {}) {
    let skills = this.getAll();

    if (category) {
      skills = skills.filter(s => s.category === category);
    }

    if (stability) {
      skills = skills.filter(s => s.stability === stability);
    }

    if (minStability) {
      const minLevel = STABILITY_LEVELS[minStability];
      if (minLevel) {
        skills = skills.filter(s => (STABILITY_LEVELS[s.stability] || 0) >= minLevel);
      }
    }

    return skills;
  }

  /**
   * Group skills by their category field.
   */
  byCategory() {
    const map = new Map();
    for (const skill of this.getAll()) {
      const cat = skill.category || 'general';
      if (!map.has(cat)) {
        map.set(cat, []);
      }
      map.get(cat).push(skill);
    }
    return map;
  }

  /**
   * Get downstream chain declarations for a skill.
   */
  getDownstreamChains(skillName) {
    const spec = this._getSpec(skillName);
    if (!spec) return [];
    return spec.chains?.compatible_downstream || [];
  }

  /**
   * Get upstream chain declarations for a skill.
   * Searches across all skills for those that list this skill as downstream.
   */
  getUpstreamChains(skillName) {
    const upstream = [];
    for (const spec of this.getAll()) {
      const downstream = spec.chains?.compatible_downstream || [];
      for (const chain of downstream) {
        if (chain.skill === skillName) {
          upstream.push({
            skill: spec.name,
            via: chain.via,
            description: chain.description
          });
        }
      }
    }
    return upstream;
  }

  /**
   * Async health check: test whether primary and fallback CLI binaries are installed.
   */
  async checkHealth() {
    const results = [];
    const checked = new Set();

    for (const spec of this.getAll()) {
      const primary = spec.requires?.primary?.command;
      const fallback = spec.requires?.fallback?.command;

      const entry = {
        skill: spec.name,
        primary: null,
        fallback: null
      };

      if (primary) {
        const cmd = process.platform === 'win32' && !primary.endsWith('.exe') ? primary + '.exe' : primary;
        if (!checked.has(cmd)) {
          checked.add(cmd);
          entry.primary = { command: primary, available: await _commandExists(cmd) };
        } else {
          entry.primary = { command: primary, available: true };
        }
      }

      if (fallback) {
        const cmd = process.platform === 'win32' && !fallback.endsWith('.exe') ? fallback + '.exe' : fallback;
        if (!checked.has(cmd)) {
          checked.add(cmd);
          entry.fallback = { command: fallback, available: await _commandExists(cmd) };
        } else {
          entry.fallback = { command: fallback, available: true };
        }
      }

      results.push(entry);
    }

    return results;
  }

  /**
   * Summarize health check results.
   */
  async healthSummary() {
    const health = await this.checkHealth();
    const total = health.length;
    const healthy = health.filter(h => h.primary?.available || h.fallback?.available).length;
    const missingPrimary = health.filter(h => h.primary && !h.primary.available).map(h => h.skill);
    const missingFallback = health.filter(h => h.fallback && !h.fallback.available).map(h => h.skill);

    return {
      total,
      healthy,
      unhealthy: total - healthy,
      missingPrimary,
      missingFallback,
      details: health
    };
  }

  /**
   * Search for skills by capability keyword across names, descriptions,
   * commands, and metadata tags.
   */
  findByCapability(keyword) {
    const term = keyword.toLowerCase();
    return this.getAll().filter(spec => {
      const haystack = [
        spec.name,
        spec.description,
        spec.execution?.command,
        spec.requires?.primary?.command,
        spec.requires?.fallback?.command,
        ...(spec.metadata?.tags || [])
      ].filter(Boolean).join(' ').toLowerCase();
      return haystack.includes(term);
    });
  }

  /**
   * Get install suggestions for missing dependencies.
   */
  async getInstallSuggestions() {
    const health = await this.checkHealth();
    const suggestions = [];
    const seen = new Set();

    for (const h of health) {
      if (h.primary && !h.primary.available) {
        const key = `${h.skill}:${h.primary.command}`;
        if (!seen.has(key)) {
          seen.add(key);
          suggestions.push({
            skill: h.skill,
            dependency: h.primary.command,
            type: 'primary',
            suggestion: INSTALL_COMMANDS[h.primary.command] || `install ${h.primary.command} via your system package manager`
          });
        }
      }
      if (h.fallback && !h.fallback.available) {
        const key = `${h.skill}:${h.fallback.command}`;
        if (!seen.has(key)) {
          seen.add(key);
          suggestions.push({
            skill: h.skill,
            dependency: h.fallback.command,
            type: 'fallback',
            suggestion: INSTALL_COMMANDS[h.fallback.command] || `install ${h.fallback.command} via your system package manager`
          });
        }
      }
    }

    return suggestions;
  }

  _getSpec(name) {
    try {
      return this.loader.load(name);
    } catch {
      return null;
    }
  }
}

/**
 * Check if a command exists in PATH using platform-specific lookup.
 */
function _commandExists(command) {
  return new Promise(resolve => {
    const isWin = process.platform === 'win32';
    const cmd = isWin ? 'where' : 'which';
    const args = isWin ? [command] : [command];

    const child = spawn(cmd, args, { stdio: 'ignore' });
    child.on('exit', code => resolve(code === 0));
    child.on('error', () => resolve(false));
  });
}

export { STABILITY_LEVELS };
