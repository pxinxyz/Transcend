/**
 * Transcend MCP Server — Bridges .skill.json specs to Model Context Protocol.
 *
 * Every skill in the skills directory is automatically exposed as an MCP tool
 * with full JSON Schema input validation. When the agent calls a tool, the
 * server routes through TranscendRuntime and returns structured JSON.
 *
 * This eliminates raw CLI calls from the agent entirely. The agent reasons
 * over typed tool schemas, not terminal text.
 *
 * Usage:
 *   node lib/mcp-server.js              # stdio mode (for Claude Desktop)
 *   node lib/mcp-server.js --http 3333  # HTTP/SSE mode
 *
 * Claude Code integration:
 *   Add to .mcp.json or claude.json:
 *   {
 *     "transcend": {
 *       "command": "node",
 *       "args": ["/path/to/transcend-nunjucks/lib/mcp-server.js"],
 *       "env": { "TRANSCEND_SKILLS_DIR": "./skills" }
 *     }
 *   }
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourcesRequestSchema,
  ReadResourceRequestSchema
} from '@modelcontextprotocol/sdk/types.js';
import { createRuntime } from './TranscendRuntime.js';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const SKILLS_DIR = process.env.TRANSCEND_SKILLS_DIR || path.join(__dirname, '..', 'skills');
const DEBUG = process.env.TRANSCEND_DEBUG === '1';

// ── Initialize Transcend Runtime ──────────────────────────

const runtime = createRuntime({
  skillsDir: SKILLS_DIR,
  debug: DEBUG,
  timeoutMs: parseInt(process.env.TRANSCEND_TIMEOUT || '60000', 10)
});

// Load all skills
let skills = [];
try {
  skills = runtime.listSkills();
  if (DEBUG) console.error(`[MCP] Loaded ${skills.length} skills from ${SKILLS_DIR}`);
} catch (err) {
  console.error(`[MCP] Failed to load skills: ${err.message}`);
  process.exit(1);
}

// ── Convert Skill Spec → MCP Tool Schema ──────────────────

function skillToMcpTool(spec) {
  // Convert Transcend input spec to JSON Schema
  const properties = {};
  const required = [];

  for (const [name, def] of Object.entries(spec.inputs)) {
    const schema = { ...def };

    // Map Transcend types to JSON Schema types
    if (schema.type === 'integer') schema.type = 'integer';
    else if (schema.type === 'array') {
      schema.type = 'array';
      if (schema.items) schema.items = { type: schema.items.type || 'string' };
    }
    else if (schema.type === 'boolean') schema.type = 'boolean';
    else schema.type = 'string';  // default

    // Clean up Transcend-specific fields not valid in JSON Schema
    delete schema.required;  // This is a boolean in transcend, not JSON Schema
    delete schema.validation;

    // Description from transcend spec
    if (def.description) schema.description = def.description;

    // Add default
    if ('default' in def) schema.default = def.default;

    properties[name] = schema;

    if (def.required === true) required.push(name);
  }

  // Build chain hints for the prompt
  const downstream = spec.chains?.compatible_downstream || [];
  const chainHint = downstream.length > 0
    ? ` Chains to: ${downstream.map(c => c.skill).join(', ')}.`
    : '';

  const fallback = spec.resilience?.fallback;
  const fallbackHint = fallback
    ? ` Fallback: ${fallback.command}.`
    : '';

  return {
    name: spec.name,
    description: `${spec.description}${chainHint}${fallbackHint}`,
    inputSchema: {
      type: 'object',
      properties,
      required
    }
  };
}

// Build tool definitions
const toolDefinitions = [];
const toolSpecs = new Map();

for (const skillMeta of skills) {
  try {
    const spec = runtime.getSpec(skillMeta.name);
    const tool = skillToMcpTool(spec);
    toolDefinitions.push(tool);
    toolSpecs.set(spec.name, spec);
  } catch (err) {
    console.error(`[MCP] Skipping skill "${skillMeta.name}": ${err.message}`);
  }
}

// ── Create MCP Server ─────────────────────────────────────

const server = new Server(
  {
    name: 'transcend',
    version: '1.2.0'
  },
  {
    capabilities: {
      tools: {},
      resources: {}
    }
  }
);

// ── Tool Handlers ─────────────────────────────────────────

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: toolDefinitions };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: inputs } = request.params;

  if (DEBUG) {
    console.error(`[MCP] Tool call: ${name}(${JSON.stringify(inputs)})`);
  }

  // Validate the skill exists
  if (!toolSpecs.has(name)) {
    return {
      content: [{
        type: 'text',
        text: JSON.stringify({
          status: 'error',
          error_type: 'skill_not_found',
          message: `Unknown skill: "${name}"`
        })
      }],
      isError: true
    };
  }

  // Execute through TranscendRuntime
  const startTime = Date.now();
  try {
    const result = await runtime.execute(name, inputs);
    const elapsed = Date.now() - startTime;

    if (DEBUG) {
      console.error(`[MCP] ${name} completed in ${elapsed}ms: ${result.status}`);
    }

    // Transcend always returns structured JSON — pass it straight through
    const outputText = JSON.stringify(result, null, 2);

    return {
      content: [{
        type: 'text',
        text: outputText
      }],
      isError: result.status === 'error'
    };

  } catch (err) {
    console.error(`[MCP] ${name} crashed: ${err.message}`);
    return {
      content: [{
        type: 'text',
        text: JSON.stringify({
          status: 'error',
          error_type: 'runtime_error',
          message: err.message
        })
      }],
      isError: true
    };
  }
});

// ── Resource Handlers (skill specs as resources) ──────────

server.setRequestHandler(ListResourcesRequestSchema, async () => {
  return {
    resources: skills.map(s => ({
      uri: `transcend://skills/${s.name}`,
      name: s.name,
      mimeType: 'application/json',
      description: s.description
    }))
  };
});

server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  const uri = request.params.uri;
  const match = uri.match(/^transcend:\/\/skills\/(.+)$/);

  if (!match) {
    throw new Error(`Unknown resource URI: ${uri}`);
  }

  const skillName = match[1];
  try {
    const spec = runtime.getSpec(skillName);
    return {
      contents: [{
        uri,
        mimeType: 'application/json',
        text: JSON.stringify(spec, null, 2)
      }]
    };
  } catch (err) {
    throw new Error(`Skill not found: ${skillName}`);
  }
});

// ── Start Server ──────────────────────────────────────────

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);

  if (DEBUG) {
    console.error('[MCP] Transcend MCP server running on stdio');
    console.error(`[MCP] Exposed ${toolDefinitions.length} tools:`);
    for (const t of toolDefinitions) {
      console.error(`  - ${t.name}`);
    }
  }
}

main().catch(err => {
  console.error('[MCP] Fatal:', err);
  process.exit(1);
});
