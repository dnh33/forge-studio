// Drive the real MCP server against the LIVE app, not a stub.
//
// The committed suite (forge-studio-mcp.test.mjs) proves the protocol against a
// stub control plane, which is fast and hermetic. This proves the other half:
// that the server actually reaches a running studio and that the tools wired to
// the new endpoints answer with real data.
import { spawn } from "node:child_process";

const child = spawn("node", ["mcp/forge-studio-mcp.mjs"], { stdio: ["pipe", "pipe", "pipe"] });
let buf = "";
const replies = [];
child.stdout.on("data", (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i).trim();
    buf = buf.slice(i + 1);
    if (line) { try { replies.push(JSON.parse(line)); } catch { /* partial */ } }
  }
});
let stderr = "";
child.stderr.on("data", (d) => { stderr += d; });

let nextId = 0;
const send = (method, params) => {
  const id = ++nextId;
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  return id;
};
const settle = async (id, ms = 45000) => {
  const t = Date.now() + ms;
  while (Date.now() < t) {
    const r = replies.find((x) => x.id === id);
    if (r) return r;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`no reply for id ${id}`);
};

let pass = 0, fail = 0;
const check = (name, ok, detail = "") => {
  if (ok) { pass++; console.log(`  ok    ${name}`); }
  else { fail++; console.log(`  FAIL  ${name}${detail ? "\n        " + String(detail).slice(0, 200) : ""}`); }
};

const init = await settle(send("initialize", { protocolVersion: "2024-11-05", capabilities: {} }));
check("initialize", init.result?.serverInfo?.name === "forge-studio", JSON.stringify(init.result).slice(0, 120));

const tools = await settle(send("tools/list", {}));
const names = (tools.result?.tools || []).map((t) => t.name);
check("tools/list reaches the live server", names.length === 15, `got ${names.length}: ${names.join(",")}`);

// A real read through the running app: the ledger for an existing set.
const dec = await settle(send("tools/call", { name: "list_decisions", arguments: { set: "portraits" } }));
const decText = dec.result?.content?.[0]?.text ?? "";
check("list_decisions returns the ledger from the live app", !dec.error && /[[{]/.test(decText), decText.slice(0, 160));

// The feature that could not work before: preview images, which exist only as artifacts.
const pre = await settle(send("tools/call", { name: "run_previews", arguments: { id: 37768167306 } }));
const preText = pre.result?.content?.[0]?.text ?? "";
check("run_previews returns real preview data URLs", preText.includes("data:image/png;base64,"), preText.slice(0, 160));

// A rejection must come back readable, not as a stack trace.
const bad = await settle(send("tools/call", { name: "save_decision", arguments: { set: "probe", file: "p.png", verdict: "brilliant" } }));
const badText = JSON.stringify(bad.result ?? bad.error ?? "");
check("a bad verdict is refused with a sentence", /verdict must be one of/.test(badText), badText.slice(0, 200));

// An undefined tool must not hang.
const undef = await settle(send("tools/call", { name: "no_such_tool", arguments: {} }));
check("an undefined tool is a readable error", JSON.stringify(undef.result ?? undef.error ?? "").length > 0);

console.log(`\n=== ${pass} ok, ${fail} failed ===`);
if (stderr.trim()) console.log("server stderr:", stderr.slice(0, 300));
child.kill();
process.exit(fail ? 1 : 0);
