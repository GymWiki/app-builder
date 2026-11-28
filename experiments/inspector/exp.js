const { chromium } = require("playwright");
const { connect } = require("./vm.js");
const fs = require("fs");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const short = (f) => (f || "").replace(/^file:\/\/\/tmp\/exp\/probe\//, "");
const isCore = (f) => /\/lib\/core\//.test(f || "");
const isUser = (n) => n?.createdByLocalProject === true;

(async () => {
  const c = await connect();
  const br = await chromium.connectOverCDP("http://127.0.0.1:9333");
  const page = br.contexts()[0].pages().find((p) => p.url().includes("8123"));
  const group = () => "g" + Math.random().toString(36).slice(2);
  await c.ext("setPubRootDirectories", { arg0: "/tmp/exp/probe" }).catch(() => {});
  await c.ext("addPubRootDirectories", { arg0: "/tmp/exp/probe" }).catch(() => {});
  console.log("pubroots", JSON.stringify((await c.ext("getPubRootDirectories")).result));

  const sem = async () => {
    await page.evaluate(() => document.querySelector("flt-semantics-placeholder")?.click());
    await sleep(300);
    return page.evaluate(() => [...document.querySelectorAll("flt-semantics")].map((e) => { const r = e.getBoundingClientRect(); return { role: e.getAttribute("role"), text: (e.getAttribute("aria-label") || e.textContent).trim(), x: r.x, y: r.y, w: r.width, h: r.height }; }));
  };
  const leaf = async (label) => {
    const all = await sem();
    const m = all.filter((n) => n.text === label && n.w > 0 && n.h > 0 && n.w < 480).sort((a, b) => a.w * a.h - b.w * b.h);
    return m[0] || all.filter((n) => n.text.split("\n").includes(label) && n.h < 200).sort((a, b) => a.w * a.h - b.w * b.h)[0];
  };
  const waitLeaf = async (label, tries = 8) => { for (let i = 0; i < tries; i++) { const n = await leaf(label); if (n) return n; await sleep(400); } return null; };
  const nav = async (label, expect) => {
    for (let attempt = 0; attempt < 3; attempt++) {
      const n = await waitLeaf(label); if (!n) continue;
      await click(n.x + n.w / 2, n.y + n.h / 2);
      if (await waitLeaf(expect, 5)) return true;
    }
    console.log("NAV FAILED", label, "->", expect); return false;
  };
  const click = async (x, y) => { await c.ext("show", { enabled: "false" }); await sleep(120); await page.mouse.click(x, y); await sleep(700); };

  async function pick(x, y) {
    const t0 = Date.now(); const ph = {};
    const lap = (k) => { ph[k] = Date.now() - t0; };
    await c.ext("show", { enabled: "true" }); lap("showOn");
    await sleep(150);
    await page.mouse.click(x, y); lap("click");
    await sleep(250);
    const g = group();
    const sel = await c.ext("getSelectedWidget", { previousSelectionId: "", objectGroup: g }); lap("selected");
    const node = sel.result;
    let chain = [];
    if (node?.valueId) {
      const r = await c.ext("getParentChain", { arg: node.valueId, objectGroup: g });
      chain = r.result.map((e) => e.node); // root → selected
    }
    lap("chain");
    await c.ext("disposeGroup", { objectGroup: g }).catch(() => {});
    await c.ext("show", { enabled: "false" }); lap("showOff");
    globalThis.__ph = ph;
    const inner = chain.slice().reverse(); // selected → root
    const user = inner.filter(isUser);
    const callSite = user.find((n) => !isCore(n.creationLocation?.file)) || null;
    return { ms: Date.now() - t0, node, inner, firstUser: user[0] || null, callSite };
  }
  const fmt = (n) => (n ? `${n.description}@${short(n.creationLocation?.file)}:${n.creationLocation?.line}` : "none");

  const results = [];
  async function check(screen, label, expectFile, lo, hi, kind = "label", fixed = null) {
    const n = fixed ? { x: 0, y: 0, w: 0, h: 0 } : await leaf(label);
    if (!n) { results.push({ screen, label, error: "no semantics node" }); console.log("MISSING", label); return; }
    const pts = fixed ? [[fixed[0][0], fixed[0][1], "center"], [fixed[1][0], fixed[1][1], "corner"]] : [[n.x + n.w / 2, n.y + n.h / 2, "center"], [n.x + 4, n.y + 4, "corner"]];
    for (const [x, y, where] of pts) {
      const r = await pick(x, y);
      const cs = r.callSite;
      const fileOk = !!cs && short(cs.creationLocation?.file) === expectFile;
      const line = cs?.creationLocation?.line;
      const lineOk = fileOk && line >= lo && line <= hi;
      const sharedHit = isCore(r.firstUser?.creationLocation?.file);
      const rec = { screen, label, kind, where, x: Math.round(x), y: Math.round(y), ms: r.ms, ph: globalThis.__ph, selected: fmt(r.node), firstUser: fmt(r.firstUser), resolved: fmt(cs), sharedHit, fileOk, lineOk, chain: r.inner.slice(0, 8).map(fmt) };
      results.push(rec);
      console.log((lineOk ? "OK  " : fileOk ? "FILE" : "BAD ") + `${screen}/${label}/${where} sel=${rec.selected} user=${rec.firstUser} resolved=${rec.resolved} ${r.ms}ms`);
    }
  }

  // ── Home
  const H = "lib/home_screen.dart";
  await check("home", "Home", H, 12, 12, "label", [[90, 28], [76, 22]]);
  await check("home", "Welcome back", H, 18, 18);
  await check("home", "Pick something to do today.", H, 20, 20);
  await check("home", "Open list", H, 22, 25);
  await check("home", "Settings", H, 27, 30);
  await check("home", "Open dialog", H, 32, 44);
  await check("home", "Featured", H, 46, 52);
  await check("home", "A highlighted card", H, 46, 52);
  // gaps
  const gaps = [["gap right of Welcome", 400, 90, 13, 55], ["gap between buttons", 300, 220, 13, 55], ["gap under card", 250, 450, 13, 55], ["appbar blank", 300, 28, 12, 12]];
  for (const [label, x, y, lo, hi] of gaps) {
    const r = await pick(x, y); const cs = r.callSite;
    const fileOk = !!cs && short(cs.creationLocation?.file) === H;
    const rec = { screen: "home", label, kind: "gap", where: "point", x, y, ms: r.ms, ph: globalThis.__ph, selected: fmt(r.node), firstUser: fmt(r.firstUser), resolved: fmt(cs), fileOk, lineOk: fileOk && cs.creationLocation.line >= lo && cs.creationLocation.line <= hi };
    results.push(rec); console.log((rec.lineOk ? "OK  " : "BAD ") + `home/${label} sel=${rec.selected} resolved=${rec.resolved}`);
  }
  // ── Dialog
  let n;
  await nav("Open dialog", "Hello there");
  await check("dialog", "Hello there", H, 36, 36);
  await check("dialog", "This is a dialog.", H, 37, 37);
  await check("dialog", "Close", H, 39, 39);
  await nav("Close", "Open dialog");
  // ── Settings
  await nav("Settings", "Notifications");
  const S = "lib/settings_screen.dart";
  await check("settings", "Settings", S, 18, 18, "label", [[110, 28], [80, 22]]);
  await check("settings", "Notifications", S, 22, 26);
  await check("settings", "Dark mode", S, 27, 31);
  await check("settings", "Version 1.0.0", S, 33, 33);
  await check("settings", "Save", S, 35, 35);
  // back
  await nav("Back", "Open list");
  // ── List
  await nav("Open list", "Item 0");
  const L = "lib/list_screen.dart";
  await check("list", "Items", L, 10, 10, "label", [[100, 28], [80, 22]]);
  await check("list", "Item 0", L, 16, 20);
  await check("list", "Details for item 1", L, 16, 20);
  await check("list", "Add item", L, 25, 25);
  // delete icon of item 2: find button semantics with no text near right edge
  const all = await sem();
  const del = all.filter((x) => x.role === "button" && x.text === "" && x.x > 400 && x.y > 150 && x.y < 400)[0];
  if (del) {
    for (const [x, y, where] of [[del.x + del.w / 2, del.y + del.h / 2, "center"], [del.x + 6, del.y + 6, "corner"]]) {
      const r = await pick(x, y); const cs = r.callSite;
      const fileOk = !!cs && short(cs.creationLocation?.file) === L;
      const rec = { screen: "list", label: "delete icon", kind: "label", where, x: Math.round(x), y: Math.round(y), ms: r.ms, ph: globalThis.__ph, selected: fmt(r.node), firstUser: fmt(r.firstUser), resolved: fmt(cs), fileOk, lineOk: fileOk && cs.creationLocation.line >= 16 && cs.creationLocation.line <= 20 };
      results.push(rec); console.log((rec.lineOk ? "OK  " : "BAD ") + `list/delete icon/${where} sel=${rec.selected} resolved=${rec.resolved}`);
    }
  }
  // scroll
  await page.mouse.move(200, 300); await page.mouse.wheel(0, 1500); await sleep(800);
  const sc = (await sem()).map((x) => x.text).filter((t) => /^Item \d+$/.test(t));
  console.log("after scroll items:", sc.join(","));
  if (sc.length) {
    const lab = sc[Math.floor(sc.length / 2)];
    await check("list-scrolled", lab, L, 16, 20);
    await check("list-scrolled", "Details for item " + lab.split(" ")[1], L, 16, 20);
  }

  fs.writeFileSync("/tmp/exp/results.json", JSON.stringify(results, null, 1));
  const labelled = results.filter((r) => !r.error);
  const tot = labelled.length, fileOk = labelled.filter((r) => r.fileOk).length, lineOk = labelled.filter((r) => r.lineOk).length;
  const by = (k) => labelled.filter((r) => r.kind === k);
  console.log(`TOTAL ${tot} taps: file ok ${fileOk} (${((fileOk / tot) * 100).toFixed(0)}%), widget line ok ${lineOk} (${((lineOk / tot) * 100).toFixed(0)}%)`);
  for (const k of ["label", "gap"]) { const b = by(k); console.log(` ${k}: ${b.length} taps, line ok ${b.filter((r) => r.lineOk).length}`); }
  const sh = labelled.filter((r) => r.sharedHit); console.log(` shared-component hits: ${sh.length}, resolved correctly ${sh.filter((r) => r.lineOk).length}`);
  const ms = labelled.map((r) => r.ms).sort((a, b) => a - b); console.log(` latency median ${ms[Math.floor(ms.length / 2)]}ms max ${ms[ms.length - 1]}ms`);
  c.ws.close(); await br.close().catch(() => {});
})().catch((e) => { console.error("ERR", e.stack || e.message); process.exit(1); });
