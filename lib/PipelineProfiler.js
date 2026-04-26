/**
 * PipelineProfiler — Execution profiling for skill runs.
 *
 * Tracks per-skill execution time, normalization step timing,
 * maintains history, and computes averages and trends.
 */

export class PipelineProfiler {
  constructor(options = {}) {
    this.maxHistory = options.maxHistory || 100;
    this._history = [];
    this._bySkill = new Map();
  }

  /**
   * Record a skill execution.
   *
   * @param {string} skillName
   * @param {number} startTime — timestamp (ms)
   * @param {number} endTime — timestamp (ms)
   * @param {Object} [options]
   * @param {Array<{step: string, duration_ms: number}>} [options.normalizationSteps]
   */
  record(skillName, startTime, endTime, options = {}) {
    const duration = endTime - startTime;
    const entry = {
      skillName,
      startTime,
      endTime,
      duration,
      normalizationSteps: options.normalizationSteps || []
    };

    this._history.push(entry);
    if (this._history.length > this.maxHistory) {
      this._history.shift();
    }

    if (!this._bySkill.has(skillName)) {
      this._bySkill.set(skillName, []);
    }
    const list = this._bySkill.get(skillName);
    list.push(entry);
    if (list.length > this.maxHistory) {
      list.shift();
    }

    return entry;
  }

  /**
   * Get execution history.
   * If skillName is provided, returns history for that skill only.
   */
  getHistory(skillName) {
    if (skillName) {
      return [...(this._bySkill.get(skillName) || [])];
    }
    return [...this._history];
  }

  /**
   * Compute average durations.
   * If skillName is provided, returns averages for that skill only.
   */
  getAverages(skillName) {
    const entries = skillName ? this._bySkill.get(skillName) || [] : this._history;
    if (entries.length === 0) {
      return {
        count: 0,
        avgDuration: 0,
        avgNormalizationTime: 0
      };
    }

    const totalDuration = entries.reduce((sum, e) => sum + e.duration, 0);
    const totalNorm = entries.reduce((sum, e) => {
      return sum + e.normalizationSteps.reduce((s, step) => s + (step.duration_ms || 0), 0);
    }, 0);

    return {
      count: entries.length,
      avgDuration: totalDuration / entries.length,
      avgNormalizationTime: totalNorm / entries.length
    };
  }

  /**
   * Compute trends: compare recent half vs older half of history.
   * If skillName is provided, returns trends for that skill only.
   */
  getTrends(skillName) {
    const entries = skillName ? this._bySkill.get(skillName) || [] : this._history;
    if (entries.length < 4) {
      return {
        durationTrend: 'insufficient_data',
        normalizationTrend: 'insufficient_data'
      };
    }

    const mid = Math.floor(entries.length / 2);
    const older = entries.slice(0, mid);
    const recent = entries.slice(mid);

    const avgOlder = older.reduce((s, e) => s + e.duration, 0) / older.length;
    const avgRecent = recent.reduce((s, e) => s + e.duration, 0) / recent.length;

    const normOlder = older.reduce((s, e) => {
      return s + e.normalizationSteps.reduce((ss, step) => ss + (step.duration_ms || 0), 0);
    }, 0) / older.length;
    const normRecent = recent.reduce((s, e) => {
      return s + e.normalizationSteps.reduce((ss, step) => ss + (step.duration_ms || 0), 0);
    }, 0) / recent.length;

    const durationTrend = avgRecent < avgOlder * 0.9 ? 'improving' : avgRecent > avgOlder * 1.1 ? 'degrading' : 'stable';
    const normalizationTrend = normRecent < normOlder * 0.9 ? 'improving' : normRecent > normOlder * 1.1 ? 'degrading' : 'stable';

    return {
      durationTrend,
      normalizationTrend,
      avgRecent,
      avgOlder,
      normRecent,
      normOlder
    };
  }

  /**
   * Clear all profiling history.
   */
  clear() {
    this._history = [];
    this._bySkill.clear();
  }

  /**
   * Get the total number of recorded executions.
   */
  get count() {
    return this._history.length;
  }
}
