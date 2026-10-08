/* Forge Studio frontend. Vanilla JS, no bundler — talks to the Rust core through
   the IPC bridge injected by `withGlobalTauri`. No API keys live in this file
   or in any browser storage: the key only ever passes through once, to Rust. */

const { invoke } = window.__TAURI__.core;
const opener = window.__TAURI__.opener;

const OWNER = "dnh33", PIPELINE = "forge-images";
const $ = (s) => document.querySelector(s);
const $$ = (s) => [...document.querySelectorAll(s)];

const state = {
  identity: null, sets: [], runs: [], renders: [],
  watched: null, model: null, orConfigured: false
};

// ------------------------------------------------------------------ utils

function toast(msg, kind = "") {
  const el = document.createElement("div");
  el.className = "toast " + kind;
  el.textContent = msg;
  $("#toasts").appendChild(el);
  setTimeout(() => el.remove(), 7000);
}

function b64(str) {
  const bytes = new TextEncoder().encode(str);
  let bin = "";
  bytes.forEach((b) => (bin += String.fromCharCode(b)));
  return btoa(bin);
}

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function ago(iso) {
  if (!iso) return "";
  const d = (Date.now() - new Date(iso).getTime()) / 1000;
  if (d < 60) return `${Math.round(d)}s ago`;
  if (d < 3600) return `${Math.round(d / 60)}m ago`;
  if (d < 86400) return `${Math.round(d / 3600)}h ago`;
  return `${Math.round(d / 86400)}d ago`;
}

async function call(cmd, args) {
  try { return await invoke(cmd, args || {}); }
  catch (e) { toast(String(e), "bad"); throw e; }
}

// ------------------------------------------------------------------ tabs

$$(".tab").forEach((t) =>
  t.addEventListener("click", () => {
    $$(".tab").forEach((x) => x.classList.remove("active"));
    $$(".panel").forEach((x) => x.classList.remove("active"));
    t.classList.add("active");
    $("#tab-" + t.dataset.tab).classList.add("active");
    if (t.dataset.tab === "gallery") loadRenders();
    if (t.dataset.tab === "runs") loadRuns();
  }));

// ------------------------------------------------------------------ items editor

function grow(el) {
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 220) + "px";
}

function addItem(id = "", seed = "", line = "") {
  const row = document.createElement("div");
  row.className = "item";
  row.innerHTML = `
    <input data-k="id" placeholder="item-id" value="${esc(id)}" spellcheck="false"
      aria-label="Item id" autocomplete="off">
    <input data-k="seed" placeholder="seed" value="${esc(seed)}" spellcheck="false"
      aria-label="Seed for this item" inputmode="numeric" autocomplete="off">
    <textarea data-k="line" rows="2" placeholder="What is in this image?" spellcheck="false"
      aria-label="Prompt line for this item">${esc(line)}</textarea>
    <div class="item-actions">
      <button class="ghost small" data-k="sharpen" title="Propose a sharper line (you approve it)"
        aria-label="Sharpen this line">&#10022;</button>
      <button class="ghost small" data-k="del" title="Remove this item" aria-label="Remove this item">&#10005;</button>
    </div>`;
  const ta = row.querySelector('[data-k="line"]');
  ta.addEventListener("input", () => grow(ta));
  requestAnimationFrame(() => grow(ta));
  row.querySelector('[data-k="del"]').addEventListener("click", () => row.remove());
  row.querySelector('[data-k="sharpen"]').addEventListener("click", () => sharpenRow(row));
  $("#items").appendChild(row);
  return row;
}

// --------------------------------------------------------------- sharpen
//
// `or_sharpen` shipped in Rust with no caller: its own comment referred to a
// button that did not exist. It rewrites one prompt line. It must never apply
// silently, so it proposes and the operator accepts.

function wordDiff(oldLine, newLine) {
  const aged = new Set(oldLine.toLowerCase().split(/\s+/));
  return newLine.split(/(\s+)/).map((piece) => {
    const bare = piece.trim().toLowerCase().replace(/[^\w'-]/g, "");
    if (!bare) return esc(piece);
    return aged.has(bare) ? esc(piece) : `<mark>${esc(piece)}</mark>`;
  }).join("");
}

async function sharpenRow(row) {
  const ta = row.querySelector('[data-k="line"]');
  const line = (ta.value || "").trim();
  if (!line) { toast("Write a line first.", "bad"); return; }
  const model = $("#orModel") ? $("#orModel").value : "";
  let proposal;
  try {
    proposal = await call("or_sharpen", { line, model });
  } catch { return; }
  proposal = String(proposal || "").trim();
  if (!proposal || proposal === line) { toast("The model proposed no change.", "bad"); return; }
  showProposal(row, ta, line, proposal);
}

function showProposal(row, ta, oldLine, proposal) {
  const old = row.nextElementSibling;
  if (old && old.classList.contains("proposal")) old.remove();
  const panel = document.createElement("div");
  panel.className = "proposal";
  panel.innerHTML = `
    <div class="was">was: ${esc(oldLine)}</div>
    <div class="new">${wordDiff(oldLine, proposal)}</div>
    <p class="fine">Highlighted words are new. Nothing is applied until you accept.</p>
    <div class="row">
      <button class="primary small" data-a="take">Accept</button>
      <button class="ghost small" data-a="drop">Discard</button>
    </div>`;
  panel.querySelector('[data-a="take"]').onclick = () => {
    ta.value = proposal;
    grow(ta);
    panel.remove();
    toast("Line replaced with the accepted rewrite.", "ok");
  };
  panel.querySelector('[data-a="drop"]').onclick = () => panel.remove();
  row.insertAdjacentElement("afterend", panel);
}
$("#addItem").addEventListener("click", () => addItem());

function collectItems() {
  const items = {};
  let n = 0;
  for (const row of $$("#items .item")) {
    const id = row.querySelector('[data-k="id"]').value.trim().replace(/\s+/g, "-");
    const seed = row.querySelector('[data-k="seed"]').value.trim();
    const line = row.querySelector('[data-k="line"]').value.trim();
    if (!id && !line) continue;
    if (!id || !line) { toast("Every item needs an id and a line.", "bad"); return null; }
    items[id] = { seed: seed === "" ? 1000 + n * 7 : Number(seed), line };
    n++;
  }
  if (n === 0) { toast("Add at least one item.", "bad"); return null; }
  return items;
}

function composeSet() {
  const slug = $("#setSlug").value.trim().toLowerCase().replace(/[^a-z0-9-]/g, "-");
  const items = collectItems();
  if (!items) return null;
  return {
    slug,
    obj: {
      name: slug || "adhoc",
      size: [Number($("#setW").value), Number($("#setH").value)],
      style: $("#setStyle").value.trim(),
      items
    }
  };
}

// ------------------------------------------------------------------ studio actions

$("#dispatchAdhoc").addEventListener("click", async () => {
  const s = composeSet();
  if (!s) return;
  s.obj.name = s.slug || "adhoc";
  $("#planHint").textContent = "Dispatching…";
  await call("dispatch_render", {
    set: "all", only: "", variants: $("#variants").value, steps: $("#steps").value,
    shards: $("#shards").value, adhoc: b64(JSON.stringify(s.obj))
  });
  toast("Render dispatched (one-off, not committed).", "ok");
  $("#planHint").textContent = `Queued ${Object.keys(s.obj.items).length} item(s).`;
  await watchNewest();
});

$("#dispatchSaved").addEventListener("click", async () => {
  const s = composeSet();
  if (!s) return;
  if (!s.slug) { toast("Give the set a slug before saving.", "bad"); return; }
  $("#planHint").textContent = "Committing set…";
  await call("save_set", { slug: s.slug, body: JSON.stringify(s.obj, null, 1) });
  toast(`Committed prompts/${s.slug}.json`, "ok");
  await call("dispatch_render", {
    set: s.slug, only: "", variants: $("#variants").value, steps: $("#steps").value,
    shards: $("#shards").value, adhoc: ""
  });
  toast("Render dispatched.", "ok");
  await watchNewest();
  await loadSets();
});

$("#pipelineLink").addEventListener("click", () =>
  opener.openUrl(`https://github.com/${OWNER}/${PIPELINE}/actions`));

// ------------------------------------------------------------------ identity

async function loadIdentity() {
  try {
    const me = await invoke("gh_identity");
    state.identity = me;
    $("#who").innerHTML =
      `<img src="${esc(me.avatar_url)}" alt=""> <b>${esc(me.login)}</b>` +
      `<span class="muted">&#183; ${esc(me.name || "")} &#183; via ${esc(me.source)}</span>`;
  } catch (e) {
    $("#who").innerHTML = `<span class="muted">not signed in</span>`;
    toast(String(e), "bad");
  }
}

// ------------------------------------------------------------------ OpenRouter

async function loadOr() {
  const s = await call("or_status");
  state.orConfigured = s.configured;
  state.model = s.model || state.model;
  $("#orBadge").textContent = s.configured ? `key from ${s.source}` : "no OpenRouter key — open Settings";
  $("#orState").textContent = s.configured
    ? `Key detected via ${s.source}. A configured key is never displayed.`
    : "No key stored yet. Paste one below, or set OPENROUTER_API_KEY in the environment.";
  $("#orForge").disabled = !s.configured;
  $("#orAsk").disabled = !s.configured;
}

async function loadOrModels() {
  let models;
  try { models = await call("or_models"); } catch { return; }
  const sel = $("#orModel");
  const cur = state.model;
  const cheap = models.filter((m) => m.prompt_price === 0)
    .sort((a, b) => b.context - a.context);
  const paid = models.filter((m) => m.prompt_price > 0)
    .sort((a, b) => (a.prompt_price + a.completion_price) - (b.prompt_price + b.completion_price));
  sel.innerHTML = "";
  const add = (group, list) => {
    if (!list.length) return;
    const og = document.createElement("optgroup");
    og.label = group;
    list.slice(0, 120).forEach((m) => {
      const o = document.createElement("option");
      o.value = m.id;
      o.textContent = `${m.id}  (${(m.context / 1000).toFixed(0)}k)`;
      if (m.id === cur) o.selected = true;
      og.appendChild(o);
    });
    sel.appendChild(og);
  };
  add("free", cheap);
  add("paid (cheapest first)", paid);
  if (!sel.value && sel.options.length) sel.selectedIndex = 0;
}

$("#gear").addEventListener("click", async () => {
  $("#settings").classList.remove("hidden");
  await loadOr();
  await loadOrModels();
  await loadControl();
  await loadUpdate();
});

async function loadControl() {
  let path;
  try {
    path = await invoke("control_descriptor");
  } catch {
    path = null;
  }
  const el = $("#ctlState");
  if (!el) return;
  el.innerHTML = path
    ? "Listening on loopback. Any local agent can drive this app through it, and the port and token are published in "
      + "<code>" + esc(path) + "</code>. Endpoints and the bundled MCP server are described in <code>docs/CONTROL.md</code>."
    : "Off. The app was started with FORGE_CONTROL=off, so nothing can drive it from outside.";
}
$("#setClose").addEventListener("click", () => $("#settings").classList.add("hidden"));
$("#setDone").addEventListener("click", () => $("#settings").classList.add("hidden"));

// ------------------------------------------------------------------- updates

async function loadUpdate() {
  const el = $("#updState");
  if (!el) return;
  el.textContent = "checking\u2026";
  $("#updInstall").classList.add("hidden");
  let r;
  try {
    r = await invoke("check_update");
  } catch (e) {
    el.textContent = "Could not reach the update service. " + (e && e.message ? e.message : e);
    return;
  }
  if (r && r.available) {
    el.innerHTML = "Version <b>" + esc(String(r.version)) + "</b> is available; you have "
      + esc(String(r.current)) + ".";
    $("#updInstall").classList.remove("hidden");
  } else {
    el.textContent = "Up to date (version " + ((r && r.current) || "?") + ").";
  }
}

$("#updCheck").addEventListener("click", loadUpdate);

$("#updInstall").addEventListener("click", async () => {
  const btn = $("#updInstall");
  const el = $("#updState");
  btn.disabled = true;
  el.textContent = "Downloading and verifying the signature\u2026";
  try {
    const msg = await invoke("install_update");
    el.textContent = msg + ". Close and reopen Forge Studio to use it.";
    toast("Update installed. Reopen the app to use it.", "ok");
  } catch (e) {
    el.textContent = "Update failed: " + (e && e.message ? e.message : e);
    toast("Update failed.", "bad");
  } finally {
    btn.disabled = false;
  }
});

$("#orSave").addEventListener("click", async () => {
  const v = $("#orKey").value.trim();
  if (!v) { toast("Paste a key first.", "bad"); return; }
  await call("or_set_key", { key: v });
  $("#orKey").value = "";               // never keep it in the field
  toast("Key stored in the OS credential store.", "ok");
  await loadOr();
  await loadOrModels();
});

$("#orClear").addEventListener("click", async () => {
  await call("or_clear_key");
  toast("OpenRouter key forgotten.", "ok");
  await loadOr();
});

$("#orVerify").addEventListener("click", async () => {
  try {
    const k = await call("or_verify");
    toast(`Key OK — "${k.label}", usage $${k.usage.toFixed(4)}${k.limit ? ` / $${k.limit}` : ""}`, "ok");
  } catch (e) { /* toast already shown */ }
});

$("#orModel").addEventListener("change", async (e) => {
  state.model = e.target.value;
  await call("or_set_model", { model: state.model });
  toast("Default model set.", "ok");
});

$("#orForge").addEventListener("click", async () => {
  const brief = $("#brief").value.trim();
  if (!brief) { toast("Write a brief first.", "bad"); return; }
  const model = state.model || $("#orModel").value;
  if (!model) { toast("Pick a model in Settings.", "bad"); return; }
  $("#ideateHint").textContent = "Thinking…";
  try {
    const json = await call("or_ideate", {
      brief, model, count: Number($("#orCount").value)
    });
    const set = JSON.parse(json);
    applySet(set);
    $("#ideateHint").textContent = `Forged ${Object.keys(set.items || {}).length} item(s) with ${model}.`;
    toast("Items forged.", "ok");
  } catch (e) {
    $("#ideateHint").textContent = "Nothing forged — see the message.";
  }
});

function applySet(set) {
  if (set.style) $("#setStyle").value = set.style;
  if (set.size && set.size.length === 2) {
    $("#setW").value = set.size[0];
    $("#setH").value = set.size[1];
  }
  if (set.name && !$("#setSlug").value) {
    $("#setSlug").value = String(set.name).toLowerCase().replace(/[^a-z0-9-]/g, "-").slice(0, 40);
  }
  $("#items").innerHTML = "";
  for (const [id, it] of Object.entries(set.items || {})) addItem(id, it.seed, it.line);
}

$("#orAsk").addEventListener("click", () => {
  $("#askModal").classList.remove("hidden");
  $("#askOut").textContent = "";
});
$("#askClose").addEventListener("click", () => $("#askModal").classList.add("hidden"));
$("#askGo").addEventListener("click", async () => {
  const q = $("#askQ").value.trim();
  if (!q) { toast("Write a question.", "bad"); return; }
  $("#askOut").textContent = "Thinking…";
  try {
    const a = await call("or_advise", {
      question: q, context: $("#askCtx").value.trim() || null, model: state.model || $("#orModel").value
    });
    $("#askOut").textContent = a;
  } catch (e) { $("#askOut").textContent = String(e); }
});

// ------------------------------------------------------------------ sets

async function loadSets() {
  state.sets = await call("list_sets");
  $("#setsCount").textContent = `(${state.sets.length})`;
  const host = $("#sets");
  host.innerHTML = "";
  if (!state.sets.length) { host.innerHTML = `<p class="hint">No sets yet.</p>`; return; }
  for (const s of state.sets) {
    const row = document.createElement("div");
    row.className = "srow";
    row.innerHTML = `
      <div class="nm">${esc(s.name)}</div>
      <div class="meta">${esc(s.slug)}.json &#183; ${s.size[0]}&#215;${s.size[1]} &#183; ${s.items} item(s)</div>
      <div class="spacer"></div>
      <button class="ghost small" data-a="load">open in studio</button>
      <button class="ghost small" data-a="render">render</button>`;
    row.querySelector('[data-a="load"]').onclick = () => openInStudio(s.slug);
    row.querySelector('[data-a="render"]').onclick = async () => {
      await call("dispatch_render", {
        set: s.slug, only: "", variants: $("#variants").value, steps: $("#steps").value,
        shards: $("#shards").value, adhoc: ""
      });
      toast(`Rendering ${s.slug}…`, "ok");
      await watchNewest();
    };
    host.appendChild(row);
  }
}

async function openInStudio(slug) {
  const set = await call("get_set", { slug });
  $("#setSlug").value = slug;
  $("#setW").value = set.size[0];
  $("#setH").value = set.size[1];
  $("#setStyle").value = set.style || "";
  $("#items").innerHTML = "";
  for (const [id, it] of Object.entries(set.items)) addItem(id, it.seed, it.line);
  $$(".tab").find((t) => t.dataset.tab === "studio").click();
  toast(`Loaded ${slug} into the studio.`, "ok");
}

$("#reloadSets").addEventListener("click", loadSets);

// ------------------------------------------------------------------ runs

function runPill(status, conclusion) {
  const cls = conclusion || status;
  return `<span class="pill ${esc(cls)}">${esc(conclusion || status)}</span>`;
}

async function loadRuns() {
  state.runs = await call("list_runs", { limit: 15 });
  const host = $("#runs");
  host.innerHTML = "";
  if (!state.runs.length) { host.innerHTML = `<p class="hint">No runs yet.</p>`; return state.runs; }
  for (const r of state.runs) {
    const row = document.createElement("div");
    row.className = "rrow";
    row.innerHTML = `
      <div class="nm">#${r.number}</div>
      ${runPill(r.status, r.conclusion)}
      <div class="meta">${esc(r.title)} &#183; ${ago(r.created_at)}</div>
      <div class="spacer"></div>
      <button class="ghost small" data-a="open">logs &#8599;</button>
      <button class="ghost small" data-a="out">output</button>`;
    row.querySelector('[data-a="open"]').onclick = () => opener.openUrl(r.html_url);
    row.querySelector('[data-a="out"]').onclick = () => showOutputs(r);
    host.appendChild(row);
  }
  const live = state.runs.some((r) => r.status !== "completed");
  $("#runDot").classList.toggle("hidden", !live);
  return state.runs;
}

$("#reloadRuns").addEventListener("click", () => loadRuns());

async function loadRunsSafe() {
  try { return await loadRuns(); } catch { return state.runs; }
}

async function watchNewest() {
  const before = state.runs.map((r) => r.id);
  for (let i = 0; i < 12; i++) {
    await new Promise((r) => setTimeout(r, 4000));
    const runs = await loadRunsSafe();
    const fresh = runs.find((r) => !before.includes(r.id));
    if (fresh) { watch(fresh.id); return; }
  }
  toast("Dispatched, but no new run appeared yet — check Runs.", "bad");
}

function watch(runId) {
  state.watched = runId;
  $("#runDot").classList.remove("hidden");
  const tick = async () => {
    let d;
    try { d = await invoke("get_run", { id: runId }); } catch { return; }
    if (d.run.status !== "completed") { setTimeout(tick, 6000); return; }
    state.watched = null;
    $("#runDot").classList.add("hidden");
    await loadRuns();
    await loadRenders().catch(() => {});
    if (d.run.conclusion === "success") await showOutputs(d.run, true);
    else toast(`Run #${d.run.number} finished: ${d.run.conclusion}.`, "bad");
  };
  setTimeout(tick, 3000);
}

// ------------------------------------------------------------------ outputs / modal

async function showOutputs(run, auto = false) {
  // the publish step lands a moment after render; retry briefly
  for (let i = 0; i < 10; i++) {
    // Tauri v2 exposes a Rust command's parameters to JS in camelCase, so
    // `run_id` on the Rust side is `runId` here. Passing run_id silently fails
    // with "missing required key runId".
    const files = await call("run_outputs", { runId: run.id });
    if (files.length) return openModal(run, files, auto);
    await new Promise((r) => setTimeout(r, 5000));
  }
  toast("Run done, but no published images found (check the logs).", "bad");
}

function openModal(run, files, auto) {
  $("#modalTitle").textContent = `Run #${run.number} produced ${files.length} image(s)`;
  $("#modalBody").innerHTML = auto
    ? "The pipeline is done. Decide on each one below, then take the download."
    : `From the <code>renders</code> branch.`;
  triage.reset(files);
  $("#modal").classList.remove("hidden");
  $("#modalGet").onclick = () => downloadUrls(files.map((f) => f.url));
  $("#modalSkip").onclick = () => $("#modal").classList.add("hidden");
  $("#modalClose").onclick = () => $("#modal").classList.add("hidden");
  $("#modalRedispatch").onclick = () => triage.redispatch();
}

// ------------------------------------------------------------------- triage
//
// A keyboard pass over a run's output. Each decision is written straight to
// `renders/<set>/<image>.decision.json` on the renders branch, beside the image
// and its sidecar, so what was kept and why is versioned with the work. This
// pass never dispatches and never rewrites a prompt; it only records.

const TRIAGE_REASONS = ["muddy", "off-style", "wrong-subject", "wrong-composition",
  "artifacts", "duplicate", "close-but-off"];
const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const triage = {
  files: [], idx: 0, set: "", tiles: [], existing: new Map(), pending: new Map(),

  reset(files) {
    this.files = files || [];
    this.idx = 0;
    this.tiles = [];
    this.existing = new Map();
    this.pending = new Map();
    this.set = (this.files[0] && this.files[0].set) || "";
    this.build();
    this.loadExisting();
    this.paint();
  },

  build() {
    const grid = $("#modalGrid");
    grid.innerHTML = "";
    this.files.slice(0, 24).forEach((f, i) => {
      const t = document.createElement("div");
      t.className = "tile";
      t.innerHTML = `<img src="${esc(f.url)}" loading="lazy" alt="">`
        + `<div class="cap">${esc(f.file)}</div><div class="badge"></div>`;
      t.querySelector("img").onclick = () => { this.idx = i; this.paint(); opener.openUrl(f.url); };
      grid.appendChild(t);
      this.tiles.push(t);
    });
  },

  async loadExisting() {
    if (!this.set) return;
    try {
      const all = await call("decisions", { set: this.set });
      Object.entries(all || {}).forEach(([path, d]) => {
        const stem = path.split("/").pop().replace(".decision.json", "");
        this.existing.set(stem, d);
      });
    } catch { /* no history is not an error for a fresh set */ }
    this.paint();
  },

  stemOf(file) { return String(file).replace(/\.(png|jpg)$/i, ""); },

  decisionFor(file) {
    const stem = this.stemOf(file);
    return this.pending.get(stem) || this.existing.get(stem) || null;
  },

  paint() {
    this.tiles.forEach((t, i) => {
      const f = this.files[i];
      if (!f) return;
      t.classList.toggle("sel", i === this.idx);
      const d = this.decisionFor(f.file);
      t.classList.remove("keep", "reject", "undecided");
      const badge = t.querySelector(".badge");
      if (!d) { badge.textContent = ""; return; }
      t.classList.add(d.verdict);
      badge.textContent = d.reason ? `${d.verdict}: ${d.reason}` : d.verdict;
    });
  },

  move(delta) {
    if (!this.files.length) return;
    this.idx = (this.idx + delta + this.files.length) % this.files.length;
    this.paint();
    const t = this.tiles[this.idx];
    if (t) t.scrollIntoView({ block: "nearest" });
  },

  async decide(verdict, reason, note) {
    const f = this.files[this.idx];
    if (!f) return;
    const stem = this.stemOf(f.file);
    this.pending.set(stem, { verdict, reason: reason || null, note: note || null });
    this.paint();
    try {
      await call("save_decision", {
        set: f.set, file: f.file, verdict, reason: reason || null, note: note || null,
      });
    } catch (e) {
      // Keep the optimistic mark but say so: the record did not land.
      toast("Could not save that decision.", "bad");
    }
  },

  rejectedKeys() {
    const keys = new Set();
    this.files.forEach((f) => {
      const d = this.decisionFor(f.file);
      if (!d || d.verdict !== "reject") return;
      const m = String(f.file).match(
        new RegExp("^" + reEsc(f.set) + "-(.+)-s\\d+\\.(png|jpg)$")
      );
      if (m) keys.add(m[1]);
    });
    return [...keys];
  },

  async redispatch() {
    const keys = this.rejectedKeys();
    if (!keys.length) { toast("Nothing marked reject.", "bad"); return; }
    const set = this.set;
    toast(`Re-rendering ${keys.length} rejected item(s) in ${set}…`, "ok");
    await call("dispatch_render", {
      set, only: keys.join(","), variants: "1", steps: "4", shards: String(keys.length), adhoc: "",
    });
    $("#modal").classList.add("hidden");
  },
};

document.addEventListener("keydown", (e) => {
  if ($("#modal").classList.contains("hidden")) return;
  const tag = (e.target && e.target.tagName) || "";
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  const k = e.key;
  if (k === "ArrowRight") { triage.move(1); e.preventDefault(); return; }
  if (k === "ArrowLeft") { triage.move(-1); e.preventDefault(); return; }
  if (k === "k") { triage.decide("keep"); return; }
  if (k === "r") { triage.decide("reject"); return; }
  if (k === "u") { triage.decide("undecided"); return; }
  if (k >= "1" && k <= "7") {
    triage.decide("reject", TRIAGE_REASONS[Number(k) - 1]);
  }
});

// ------------------------------------------------------------------ gallery

async function loadRenders() {
  state.renders = await call("list_renders");
  $("#galleryCount").textContent = `(${state.renders.length})`;
  const host = $("#gallery");
  host.innerHTML = "";
  if (!state.renders.length) {
    host.innerHTML = `<p class="hint">Nothing published yet. Run a render first.</p>`;
    return;
  }
  const bySet = {};
  state.renders.forEach((r) => (bySet[r.set] ??= []).push(r));
  for (const [set, files] of Object.entries(bySet)) {
    const h = document.createElement("div");
    h.className = "section-label";
    h.textContent = `${set} — ${files.length}`;
    host.appendChild(h);
    const g = document.createElement("div");
    g.className = "gallery";
    files.forEach((f) => {
      const t = document.createElement("div");
      t.className = "tile";
      t.innerHTML = `<input type="checkbox" data-url="${esc(f.url)}">
        <img src="${esc(f.url)}" loading="lazy" alt="">
        <button class="open">&#8599;</button>
        <div class="cap">${esc(f.file)}</div>`;
      t.querySelector("img").onclick = () => opener.openUrl(f.url);
      t.querySelector(".open").onclick = () => opener.openUrl(f.url);
      g.appendChild(t);
    });
    host.appendChild(g);
  }
}

$("#selectAll").addEventListener("click", () => {
  const boxes = $$("#gallery input[type=checkbox]");
  const all = boxes.every((b) => b.checked);
  boxes.forEach((b) => (b.checked = !all));
});

$("#downloadSel").addEventListener("click", async () => {
  const checked = $$("#gallery input[type=checkbox]:checked");
  if (!checked.length) { toast("Nothing selected.", "bad"); return; }
  await downloadUrls(checked.map((b) => b.dataset.url));
});

async function downloadUrls(urls) {
  const dir = await call("pick_folder");
  if (!dir) return;
  let ok = 0;
  for (const u of urls) {
    try { await call("download_image", { url: u, dir }); ok++; }
    catch (e) { toast(`Failed: ${u.split("/").pop()}`, "bad"); }
  }
  toast(`Downloaded ${ok}/${urls.length} to ${dir}`, "ok");
}

// ------------------------------------------------------------------ boot

(async function boot() {
  addItem("subject-1", 1001, "");
  await loadIdentity().catch(() => {});
  await loadSets().catch(() => {});
  await loadRunsSafe();
  loadOr().catch(() => {});
  const live = state.runs.find((r) => r.status !== "completed");
  if (live) watch(live.id);
  loadRenders().catch(() => {});
  setInterval(() => { if (!state.watched) loadRunsSafe(); }, 20000);
})();
