#!/usr/bin/env node
/**
 * Forge Studio MCP server.
 *
 * Speaks the Model Context Protocol over stdio and forwards every call to the
 * running Forge Studio app's loopback control plane. Nothing is authenticated
 * here: this process reads the app's own descriptor file (port + token) from the
 * app's config directory, so it can only talk to a Forge Studio that is running
 * as the same user on the same machine.
 *
 * The app must be running. If it is not, every tool returns a clear error saying
 * so rather than hanging.
 *
 * Usage:  node mcp/forge-studio-mcp.mjs
 * Configure it as a stdio MCP server in your agent (e.g. Hermes `mcp.servers`).
 */

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const SERVER_INFO = { name: "forge-studio", version: "0.1.0" };
const PROTOCOL = "2024-11-05";

function descriptorPaths() {
  const p = [];
  if (process.env.FORGE_CONTROL_FILE) p.push(process.env.FORGE_CONTROL_FILE);
  if (process.env.APPDATA) p.push(join(process.env.APPDATA, "forge-studio", "control.json"));
  if (process.env.XDG_CONFIG_HOME) p.push(join(process.env.XDG_CONFIG_HOME, "forge-studio", "control.json"));
  p.push(join(homedir(), ".config", "forge-studio", "control.json"));
  return p;
}

function discover() {
  for (const path of descriptorPaths()) {
    try {
      const d = JSON.parse(readFileSync(path, "utf8"));
      if (d && d.port && d.token) return d;
    } catch { /* try the next candidate */ }
  }
  return null;
}

const NOT_RUNNING =
  "Forge Studio is not running (no control.json found, or the app was started with " +
  "FORGE_CONTROL=off). Start the app, then retry.";

async function api(method, path, body) {
  const d = discover();
  if (!d) throw new Error(NOT_RUNNING);
  const res = await fetch(`http://127.0.0.1:${d.port}${path}`, {
    method,
    headers: {
      "Authorization": `Bearer ${d.token}`,
      "Content-Type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let parsed;
  try { parsed = JSON.parse(text); } catch { parsed = { raw: text }; }
  if (!res.ok) {
    const msg = parsed && parsed.error ? parsed.error : `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return parsed;
}

const TOOLS = [
  {
    name: "studio_status",
    description: "Check the studio is reachable and report the signed-in GitHub user and whether an OpenRouter key is configured.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "list_sets",
    description: "List the prompt sets committed to the pipeline repository (forge-images), with size and item count.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "get_set",
    description: "Read one prompt set by slug: its canvas size, shared style block, and every item line and seed.",
    inputSchema: {
      type: "object",
      properties: { slug: { type: "string", description: "Set slug, e.g. 'portraits'" } },
      required: ["slug"], additionalProperties: false,
    },
  },
  {
    name: "save_set",
    description: "Create or overwrite prompts/<slug>.json in the pipeline repository. The body is validated as a prompt set first.",
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string" },
        set: { type: "object", description: "Prompt set: {name,size:[w,h],style,items:{id:{seed,line}}}" },
      },
      required: ["slug", "set"], additionalProperties: false,
    },
  },
  {
    name: "dispatch_render",
    description: "Start a render run on GitHub Actions. Use set='all' with a base64 `adhoc` set to render a one-off without committing anything.",
    inputSchema: {
      type: "object",
      properties: {
        set: { type: "string", description: "A set slug, or 'all'", default: "all" },
        only: { type: "string", description: "Comma-separated item ids" },
        variants: { type: "string", description: "Seeds per item", default: "2" },
        steps: { type: "string", default: "4" },
        shards: { type: "string", description: "Parallel jobs", default: "8" },
        adhoc: { type: "string", description: "base64-encoded one-off set JSON" },
      },
      additionalProperties: false,
    },
  },
  {
    name: "list_runs",
    description: "List the most recent pipeline runs with status and conclusion.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "get_run",
    description: "Get one run with its jobs and their individual conclusions.",
    inputSchema: {
      type: "object",
      properties: { id: { type: "number", description: "Run id" } },
      required: ["id"], additionalProperties: false,
    },
  },
  {
    name: "run_outputs",
    description: "The exact images a finished run produced, by diffing the run's publish commit on the renders branch. Empty until the run has published.",
    inputSchema: {
      type: "object",
      properties: { id: { type: "number", description: "Run id" } },
      required: ["id"], additionalProperties: false,
    },
  },
  {
    name: "list_renders",
    description: "Everything ever published on the renders branch, with direct image URLs.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "download_images",
    description: "Download images by URL into a local directory. Returns the saved paths and any failures.",
    inputSchema: {
      type: "object",
      properties: {
        urls: { type: "array", items: { type: "string" } },
        dir: { type: "string", description: "Absolute destination directory" },
      },
      required: ["urls", "dir"], additionalProperties: false,
    },
  },
  {
    name: "ideate",
    description: "Turn a short brief into a coherent prompt set using the configured OpenRouter model. Returns set JSON ready for save_set or dispatch_render.",
    inputSchema: {
      type: "object",
      properties: {
        brief: { type: "string" },
        model: { type: "string", description: "OpenRouter model id; empty uses the configured default" },
        count: { type: "number", default: 4 },
      },
      required: ["brief"], additionalProperties: false,
    },
  },
  {
    name: "advise",
    description: "Ask the configured advisor model a question, optionally with context. Same routing as the CLI /advisor.",
    inputSchema: {
      type: "object",
      properties: {
        question: { type: "string" },
        context: { type: "string" },
        model: { type: "string" },
      },
      required: ["question"], additionalProperties: false,
    },
  },
];

async function callTool(name, args) {
  const a = args || {};
  switch (name) {
    case "studio_status": return api("GET", "/status");
    case "list_sets": return api("GET", "/sets");
    case "get_set": return api("GET", `/set/${encodeURIComponent(a.slug)}`);
    case "save_set": return api("PUT", `/set/${encodeURIComponent(a.slug)}`, a.set);
    case "dispatch_render": return api("POST", "/dispatch", a);
    case "list_runs": return api("GET", "/runs");
    case "get_run": return api("GET", `/run/${a.id}`);
    case "run_outputs": return api("GET", `/run/${a.id}/outputs`);
    case "list_renders": return api("GET", "/renders");
    case "download_images": return api("POST", "/download", a);
    case "ideate": return api("POST", "/ideate", a);
    case "advise": return api("POST", "/advise", a);
    default: throw new Error(`unknown tool: ${name}`);
  }
}

function send(msg) {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

// In-flight requests are tracked so a closing stdin does not kill their replies.
const pending = new Set();
function track(promise) {
  pending.add(promise);
  promise.finally(() => pending.delete(promise));
  return promise;
}

async function handle(req) {
  const { id, method, params } = req;
  const isNotification = id === undefined || id === null;
  try {
    if (method === "initialize") {
      send({ jsonrpc: "2.0", id, result: {
        protocolVersion: PROTOCOL,
        capabilities: { tools: {} },
        serverInfo: SERVER_INFO,
      }});
      return;
    }
    if (method === "notifications/initialized" || method === "initialized") return;
    if (method === "ping") { send({ jsonrpc: "2.0", id, result: {} }); return; }
    if (method === "tools/list") { send({ jsonrpc: "2.0", id, result: { tools: TOOLS } }); return; }
    if (method === "tools/call") {
      const name = params && params.name;
      const args = (params && params.arguments) || {};
      try {
        const result = await callTool(name, args);
        send({ jsonrpc: "2.0", id, result: {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        }});
      } catch (e) {
        send({ jsonrpc: "2.0", id, result: {
          content: [{ type: "text", text: String(e && e.message ? e.message : e) }],
          isError: true,
        }});
      }
      return;
    }
    if (!isNotification) {
      send({ jsonrpc: "2.0", id, error: { code: -32601, message: `method not found: ${method}` } });
    }
  } catch (e) {
    if (!isNotification) {
      send({ jsonrpc: "2.0", id, error: { code: -32603, message: String(e) } });
    }
  }
}

let buf = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buf += chunk;
  let nl;
  while ((nl = buf.indexOf("\n")) !== -1) {
    const line = buf.slice(0, nl).trim();
    buf = buf.slice(nl + 1);
    if (!line) continue;
    let msg;
    try { msg = JSON.parse(line); } catch { continue; }
    track(handle(msg));
  }
});
process.stdin.on("end", async () => {
  // Drain before exiting, so a client that closes the pipe straight after a call
  // still receives that call's reply.
  const deadline = Date.now() + 30_000;
  while (pending.size && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 10));
  }
  process.exit(0);
});
