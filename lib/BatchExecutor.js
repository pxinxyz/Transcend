/**
 * BatchExecutor — Parallel skill execution with caching, deduplication,
 * token budget enforcement, and error isolation.
 */

import { createHash } from 'crypto';

export class BatchExecutor {
  constructor(options = {}) {
    this.runtime = options.runtime;
    this.maxConcurrency = options.maxConcurrency || 4;
    this.tokenBudget = options.tokenBudget || null;
    this.cache = new Map();
    this.tokenUsed = 0;
  }

  /**
   * Execute a batch of skill tasks in parallel.
   *
   * @param {Array<{skillName: string, inputs: Object, id?: string}>} tasks
   * @returns {Promise<Array<{id: string, status: string, result?: Object, error?: Error}>>}
   */
  async executeBatch(tasks) {
    if (!Array.isArray(tasks)) {
      throw new TypeError('tasks must be an array');
    }

    // Deduplicate tasks by key
    const taskMap = new Map();
    const order = [];

    for (const task of tasks) {
      const key = this._taskKey(task);
      if (!taskMap.has(key)) {
        taskMap.set(key, { ...task, _keys: [key] });
        order.push(key);
      } else {
        const existing = taskMap.get(key);
        existing._keys.push(key);
      }
    }

    const uniqueTasks = order.map(k => taskMap.get(k));
    const results = new Map();

    // Process with limited concurrency
    let index = 0;

    const runNext = async () => {
      while (index < uniqueTasks.length) {
        const task = uniqueTasks[index++];

        // Check token budget
        const estimatedTokens = this._estimateTokens(task.inputs);
        if (this.tokenBudget !== null && this.tokenUsed + estimatedTokens > this.tokenBudget) {
          for (const key of task._keys) {
            results.set(key, {
              id: task.id || key,
              status: 'budget_exceeded',
              error: new Error(`Token budget exceeded: ${this.tokenUsed} + ${estimatedTokens} > ${this.tokenBudget}`)
            });
          }
          continue;
        }

        // Check cache
        const cacheKey = this._cacheKey(task);
        if (this.cache.has(cacheKey)) {
          const cached = this.cache.get(cacheKey);
          for (const key of task._keys) {
            results.set(key, {
              id: task.id || key,
              status: cached.status,
              result: cached.result,
              cached: true
            });
          }
          continue;
        }

        // Execute
        try {
          if (this.tokenBudget !== null) {
            this.tokenUsed += estimatedTokens;
          }

          const result = await this.runtime.execute(task.skillName, task.inputs || {});

          const envelope = {
            id: task.id || cacheKey,
            status: result.status === 'error' ? 'error' : 'ok',
            result
          };

          this.cache.set(cacheKey, envelope);

          for (const key of task._keys) {
            results.set(key, { ...envelope, id: task.id || key });
          }
        } catch (err) {
          const envelope = {
            id: task.id || cacheKey,
            status: 'error',
            error: err
          };

          for (const key of task._keys) {
            results.set(key, { ...envelope, id: task.id || key });
          }
        }
      }
    };

    const workers = [];
    const concurrency = Math.min(this.maxConcurrency, uniqueTasks.length || 1);
    for (let i = 0; i < concurrency; i++) {
      workers.push(runNext());
    }
    await Promise.all(workers);

    // Return results in original task order
    return tasks.map(task => {
      const key = this._taskKey(task);
      return results.get(key);
    });
  }

  /**
   * Clear the result cache.
   */
  clearCache() {
    this.cache.clear();
  }

  /**
   * Reset token usage counter.
   */
  resetTokenBudget() {
    this.tokenUsed = 0;
  }

  _taskKey(task) {
    return `${task.skillName}:${JSON.stringify(task.inputs || {})}`;
  }

  _cacheKey(task) {
    return createHash('sha256').update(this._taskKey(task)).digest('hex');
  }

  _estimateTokens(inputs) {
    if (!inputs) return 0;
    const json = JSON.stringify(inputs);
    // Very rough heuristic: ~4 chars per token
    return Math.ceil(json.length / 4);
  }
}
