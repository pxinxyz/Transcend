/**
 * CommandExecutor — Runs CLI tools as subprocesses with full control.
 *
 * Handles: spawning, stdout/stderr streaming, timeout enforcement,
 * exit code interpretation via the skill's exit_code_map, and
 * stdin piping for skills that accept data via stdin (like find_replace
 * which pipes the files array to avoid ARG_MAX limits).
 */

import { spawn } from 'child_process';

export class CommandExecutor {
  constructor(options = {}) {
    this.defaultTimeout = options.timeoutMs || 30000;
    this.debug = options.debug ?? false;
    this.commandCache = new Map();
  }

  /**
   * Execute a command with the given arguments.
   *
   * @param {Object} options
   * @param {string} options.command — executable name or path
   * @param {string[]} options.args — CLI argument strings
   * @param {number} [options.timeoutMs] — per-execution timeout override
   * @param {string} [options.stdinData] — data to pipe into stdin (JSON string, etc.)
   * @param {Object} [options.exitCodeMap] — skill-defined exit code meanings
   * @param {string} [options.cwd] — working directory
   * @returns {Promise<ExecutionResult>}
   */
  async execute({
    command,
    args,
    timeoutMs,
    stdinData,
    exitCodeMap,
    cwd
  }) {
    const timeout = timeoutMs || this.defaultTimeout;

    if (this.debug) {
      const cmdLine = [command, ...args].map(a => a.includes(' ') ? `"${a}"` : a).join(' ');
      console.error(`[CommandExecutor] $ ${cmdLine}`);
      if (stdinData) {
        console.error(`[CommandExecutor] stdin: ${stdinData.substring(0, 200)}...`);
      }
    }

    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        cwd: cwd || process.cwd(),
        stdio: stdinData ? ['pipe', 'pipe', 'pipe'] : [null, 'pipe', 'pipe'],
        // No shell: true — we pass args directly for security
        shell: false,
        windowsHide: true  // Hide console window on Windows
      });

      const stdoutChunks = [];
      const stderrChunks = [];
      let timedOut = false;

      // Handle stdout
      child.stdout.on('data', chunk => {
        stdoutChunks.push(chunk);
      });

      // Handle stderr
      child.stderr.on('data', chunk => {
        stderrChunks.push(chunk);
      });

      // Write stdin data if provided
      if (stdinData) {
        child.stdin.write(stdinData, 'utf-8');
        child.stdin.end();
      }

      // Timeout handling
      const timer = setTimeout(() => {
        timedOut = true;
        child.kill('SIGTERM');

        // Force kill after grace period
        setTimeout(() => {
          if (!child.killed) {
            child.kill('SIGKILL');
          }
        }, 5000);
      }, timeout);

      // Process exit
      child.on('exit', (code, signal) => {
        clearTimeout(timer);

        const stdout = Buffer.concat(stdoutChunks).toString('utf-8');
        const stderr = Buffer.concat(stderrChunks).toString('utf-8');

        const result = new ExecutionResult({
          exitCode: code,
          signal,
          stdout,
          stderr,
          timedOut,
          exitCodeMap,
          command,
          args
        });

        if (this.debug) {
          console.error(`[CommandExecutor] exit=${code} signal=${signal} timedOut=${timedOut}`);
          console.error(`[CommandExecutor] stdout length: ${stdout.length}`);
          console.error(`[CommandExecutor] stderr length: ${stderr.length}`);
        }

        resolve(result);
      });

      child.on('error', err => {
        clearTimeout(timer);

        // Command not found
        if (err.code === 'ENOENT') {
          resolve(new ExecutionResult({
            exitCode: null,
            signal: null,
            stdout: '',
            stderr: `Command not found: ${command}`,
            timedOut: false,
            exitCodeMap,
            command,
            args,
            notFound: true
          }));
          return;
        }

        reject(new ExecutionError(
          `Failed to spawn "${command}": ${err.message}`,
          err.code
        ));
      });
    });
  }

  /**
   * Quick check if a command exists in PATH.
   */
  async checkCommand(command) {
    if (this.commandCache.has(command)) {
      return this.commandCache.get(command);
    }

    const available = await new Promise(resolve => {
      const child = spawn(
        process.platform === 'win32' ? 'where' : 'which',
        [command],
        { stdio: 'ignore' }
      );
      child.on('exit', code => resolve(code === 0));
      child.on('error', () => resolve(false));
    });

    this.commandCache.set(command, available);
    return available;
  }
}

/**
 * Represents the result of a subprocess execution.
 */
export class ExecutionResult {
  constructor({
    exitCode,
    signal,
    stdout,
    stderr,
    timedOut,
    exitCodeMap,
    command,
    args,
    notFound = false
  }) {
    this.exitCode = exitCode;
    this.signal = signal;
    this.stdout = stdout;
    this.stderr = stderr;
    this.timedOut = timedOut;
    this.exitCodeMap = exitCodeMap || {};
    this.command = command;
    this.args = args;
    this.notFound = notFound;
  }

  /**
   * Get the semantic meaning of the exit code from the skill spec.
   * Returns null if no mapping exists.
   */
  get exitStatus() {
    if (this.notFound) return 'command_not_found';
    if (this.timedOut) return 'timeout';
    if (this.signal) return `signal_${this.signal}`;
    const mapped = this.exitCodeMap[String(this.exitCode)];
    return mapped || 'unknown';
  }

  /**
   * True if the process exited with code 0.
   */
  get success() {
    return this.exitCode === 0 && !this.timedOut && !this.notFound;
  }

  /**
   * True if the command was not found in PATH.
   */
  get commandNotFound() {
    return this.notFound;
  }

  /**
   * Parse stdout as NDJSON (newline-delimited JSON).
   * Returns array of parsed JSON objects, skipping empty lines.
   */
  parseNdjson() {
    const lines = this.stdout.split('\n').filter(l => l.trim().length > 0);
    const results = [];
    for (const line of lines) {
      try {
        results.push(JSON.parse(line));
      } catch {
        // Skip unparsable lines
      }
    }
    return results;
  }

  /**
   * Parse stdout as a single JSON document.
   */
  parseJson() {
    const trimmed = this.stdout.trim();
    if (!trimmed) return null;
    try {
      return JSON.parse(trimmed);
    } catch (err) {
      throw new ExecutionError(
        `Failed to parse stdout as JSON: ${err.message}\n` +
        `First 200 chars: ${trimmed.substring(0, 200)}`
      );
    }
  }

  toString() {
    const cmd = [this.command, ...this.args].join(' ');
    return `ExecutionResult(command="${cmd}", exit=${this.exitCode}, status=${this.exitStatus})`;
  }
}

export class ExecutionError extends Error {
  constructor(message, code = 'EXECUTION_ERROR') {
    super(message);
    this.name = 'ExecutionError';
    this.code = code;
  }
}
