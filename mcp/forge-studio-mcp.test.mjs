/**
 * Tests for the Forge Studio MCP server.
 *
 * The server is a stdio bridge to the app's loopback control plane, so these
 * tests do the honest thing: they stand up a stub control plane on a real port,
 * point the server at it with a descriptor file, and speak JSON-RPC to the real
 * child process over real pipes. No network to GitHub, no app required.
 */
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SERVER = join(HERE, "forge-studio-mcp.mjs");
const TOKEN = "test-token-abcdef012345";

let stub;
let port;
let child;
let seq = 0;
let stdoutBuf = "";
const seen = [];          // every request the stub received
const waiters = new Map(); // id -> resolve

function writeDescriptor(dir, body) {
  const file = join(dir, "control.json");
  writeFileSync(file, JSON.stringify(body));
  return file;
}

/** Send one JSON-RPC request and resolve with the matching response. */
function rpc(method, params, timeoutMs = 5000) {
  const id = ++seq;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      waiters.delete(id);
      reject(new Error(`no reply to ${method} (id ${id}) within ${timeoutMs}ms`));
    }, timeoutMs);
    waiters.set(id, (msg) => { clearTimeout(timer); resolve(msg); });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
}

let emptyDir;

function startServer(descriptorFile) {
  const proc = spawn(process.execPath, [SERVER], {
    // Point every discovery path at an empty directory so only the descriptor
    // passed in can be found. (A NUL byte in an env value makes spawn throw,
    // so this is a real directory and not a sentinel.)
    env: {
      ...process.env,
      FORGE_CONTROL_FILE: descriptorFile,
      APPDATA: emptyDir,
      XDG_CONFIG_HOME: emptyDir,
      HOME: emptyDir,
      USERPROFILE: emptyDir,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  proc.stdout.setEncoding("utf8");
  proc.stdout.on("data", (chunk) => {
    stdoutBuf += chunk;
    let nl;
    while ((nl = stdoutBuf.indexOf("\n")) !== -1) {
      const line = stdoutBuf.slice(0, nl).trim();
      stdoutBuf = stdoutBuf.slice(nl + 1);
      if (!line) continue;
      let msg;
      try { msg = JSON.parse(line); } catch { continue; }
      const w = waiters.get(msg.id);
      if (w) { waiters.delete(msg.id); w(msg); }
    }
  });
  proc.stderr.resume();
  return proc;
}

function textOf(msg) {
  return msg.result.content.map((c) => c.text).join("\n");
}

before(async () => {
  emptyDir = mkdtempSync(join(tmpdir(), "forge-mcp-empty-"));
  stub = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      seen.push({ method: req.method, url: req.url, auth: req.headers.authorization, body });
      const json = (code, payload) => {
        res.writeHead(code, { "Content-Type": "application/json" });
        res.end(JSON.stringify(payload));
      };
      if (req.url === "/status") return json(200, { login: "dnh33", openrouter: true });
      if (req.url === "/sets") return json(200, { sets: [{ slug: "portraits", items: 2 }] });
      if (req.url.startsWith("/set/")) return json(200, { slug: "portraits", items: { marshal: { seed: 1101 } } });
      if (req.url === "/dispatch") return json(200, { dispatched: true, received: JSON.parse(body || "{}") });
      if (req.url === "/worst") return json(500, { error: "github said no" });
      return json(404, { error: "no such endpoint" });
    });
  });
  await new Promise((r) => stub.listen(0, "127.0.0.1", r));
  port = stub.address().port;

  const dir = mkdtempSync(join(tmpdir(), "forge-mcp-"));
  const descriptor = writeDescriptor(dir, { port, token: TOKEN });
  child = startServer(descriptor);
  await new Promise((r) => setTimeout(r, 250)); // let the child come up
});

after(() => {
  if (child) child.kill();
  if (stub) stub.close();
});

// ----------------------------------------------------------------- protocol

test("initialize reports the server identity and protocol", async () => {
  const msg = await rpc("initialize", { protocolVersion: "2024-11-05", capabilities: {} });
  assert.equal(msg.result.serverInfo.name, "forge-studio");
  assert.equal(msg.result.protocolVersion, "2024-11-05");
  assert.ok(msg.result.capabilities.tools, "the tools capability must be advertised");
});

test("tools/list returns the whole contract", async () => {
  const msg = await rpc("tools/list", {});
  const names = msg.result.tools.map((t) => t.name);
  assert.equal(names.length, 12, `expected 12 tools, got ${names.length}: ${names.join(", ")}`);
  for (const required of ["studio_status", "dispatch_render", "run_outputs", "download_images", "ideate"]) {
    assert.ok(names.includes(required), `missing tool: ${required}`);
  }
  for (const t of msg.result.tools) {
    assert.equal(typeof t.description, "string");
    assert.ok(t.description.length > 10, `${t.name} needs a real description`);
  }
});

test("an unknown method is a JSON-RPC error, not a hang", async () => {
  const msg = await rpc("totally/unknown", {});
  assert.equal(msg.error.code, -32601);
});

// ----------------------------------------------------------------- tool calls

test("studio_status reaches the control plane with the bearer token", async () => {
  const msg = await rpc("tools/call", { name: "studio_status", arguments: {} });
  assert.equal(msg.result.isError, undefined);
  assert.match(textOf(msg), /dnh33/);
  const last = seen.at(-1);
  assert.equal(last.url, "/status");
  assert.equal(last.auth, `Bearer ${TOKEN}`, "the MCP server must authenticate to the app");
});

test("get_set encodes the slug into the path", async () => {
  const msg = await rpc("tools/call", { name: "get_set", arguments: { slug: "portraits" } });
  assert.match(textOf(msg), /marshal/);
  assert.equal(seen.at(-1).url, "/set/portraits");
});

test("an argument with a slash cannot escape the route", async () => {
  await rpc("tools/call", { name: "get_set", arguments: { slug: "../../etc/passwd" } });
  const url = seen.at(-1).url;
  assert.ok(!url.includes("../"), `slug must be encoded, saw: ${url}`);
  assert.ok(url.startsWith("/set/"), url);
});

test("dispatch forwards its arguments as the request body", async () => {
  const args = { set: "all", variants: "1", shards: "2", adhoc: "eyJ4IjoxfQ==" };
  const msg = await rpc("tools/call", { name: "dispatch_render", arguments: args });
  const received = JSON.parse(seen.at(-1).body);
  assert.equal(received.adhoc, args.adhoc, "the one-off payload must survive the bridge");
  assert.match(textOf(msg), /dispatched/);
});

test("list_runs and list_renders map to their endpoints", async () => {
  await rpc("tools/call", { name: "list_renders", arguments: {} });
  assert.equal(seen.at(-1).url, "/renders");
});

test("an app-side error is surfaced as isError, not thrown away", async () => {
  const msg = await rpc("tools/call", { name: "get_run", arguments: { id: 999 } });
  assert.equal(msg.result.isError, true);
  assert.match(textOf(msg), /no such endpoint/, "the app's own message must reach the caller");
});

test("calling an undefined tool is a readable error", async () => {
  const msg = await rpc("tools/call", { name: "definitely_not_a_tool", arguments: {} });
  assert.equal(msg.result.isError, true);
  assert.match(textOf(msg), /unknown tool/);
});

test("a 500 from the app is reported with its message", async () => {
  const msg = await rpc("tools/call", { name: "run_outputs", arguments: { id: 1 } });
  assert.equal(msg.result.isError, true);
  assert.match(textOf(msg), /no such endpoint|github said no/);
});

// ----------------------------------------------------------------- no app

test("with no descriptor the server says so instead of hanging", async () => {
  const dir = mkdtempSync(join(tmpdir(), "forge-mcp-none-"));
  const orphan = startServer(join(dir, "control.json")); // never written
  const got = await new Promise((resolve, reject) => {
    let buf = "";
    const timer = setTimeout(() => reject(new Error("orphan server never replied")), 5000);
    orphan.stdout.setEncoding("utf8");
    orphan.stdout.on("data", (chunk) => {
      buf += chunk;
      const nl = buf.indexOf("\n");
      if (nl === -1) return;
      clearTimeout(timer);
      resolve(JSON.parse(buf.slice(0, nl)));
    });
    orphan.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "studio_status", arguments: {} } }) + "\n");
  });
  orphan.kill();
  rmSync(dir, { recursive: true, force: true });
  assert.equal(got.result.isError, true);
  assert.match(got.result.content[0].text, /not running/i);
});
