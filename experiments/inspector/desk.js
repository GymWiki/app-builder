const { execSync } = require("child_process");
const fs = require("fs");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const short = (f) => (f || "").replace(/^file:\/\/\/tmp\/exp\/probe\//, "");
const WS = fs.readFileSync("/tmp/exp/lrun.log", "utf8").match(/VM Service on Linux is available at: http:\/\/(\S+?)\/?\s/)[1];
async function connectWs() {
  const ws = new WebSocket(`ws://${WS.replace(/\/$/, "")}/ws`);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0; const pend = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { const [res, rej] = pend.get(m.id); pend.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error).slice(0, 300))) : res(m.result); } };
  const call = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pend.set(i, [res, rej]); ws.send(JSON.stringify({ id: i, method, params })); });
  const vm = await call("getVM"); const iso = vm.isolates[0].id;
  return { ws, ext: (n, a = {}) => call("ext.flutter.inspector." + n, { isolateId: iso, ...a }) };
}
const xd = (cmd) => execSync(`DISPLAY=:99 xdotool ${cmd}`).toString();
(async () => {
  const c = await connectWs();
  const wid = xd("search --onlyvisible --name probe").trim().split("\n")[0];
  const geo = xd(`getwindowgeometry ${wid}`); const pos = geo.match(/Position: (\d+),(\d+)/); const [ox, oy] = [Number(pos[1]), Number(pos[2])];
  console.log("window", geo.replace(/\n/g, " "));
  await c.ext("addPubRootDirectories", { arg0: "/tmp/exp/probe" });
  async function pick(x, y) {
    await c.ext("show", { enabled: "true" }); await sleep(300);
    xd(`mousemove ${ox + x} ${oy + y}`); await sleep(100); xd("click 1"); await sleep(500);
    const g = "g" + Math.random();
    const ss = await c.ext("getSelectedSummaryWidget", { previousSelectionId: "", objectGroup: g });
    const tree = await c.ext("getRootWidgetSummaryTree", { objectGroup: g });
    const path = []; const find = (n, p) => { const q = [...p, n]; if (n.valueId === ss.result?.valueId) { path.push(...q); return true; } return (n.children || []).some((ch) => find(ch, q)); };
    find(tree.result, []);
    await c.ext("show", { enabled: "false" });
    const site = path.filter((n) => n.createdByLocalProject).reverse().find((n) => !/\/lib\/core\//.test(n.creationLocation.file));
    return site ? { d: site.description, f: short(site.creationLocation.file), l: site.creationLocation.line } : null;
  }
  const H = "lib/home_screen.dart";
  const T = [["Home", [90, 28], [76, 22], 12, 12], ["Welcome back", [100, 92], [20, 76], 18, 18], ["Pick something", [100, 130], [20, 122], 20, 20], ["Open list", [68, 180], [20, 168], 22, 25], ["Settings", [66, 224], [20, 212], 27, 30], ["Open dialog", [78, 268], [20, 256], 32, 44], ["Featured", [100, 330], [20, 312], 46, 52], ["highlighted card", [100, 352], [20, 372], 46, 52], ["gap", [400, 90], [300, 220], 13, 55], ["gap2", [250, 450], [300, 28], 12, 55]];
  let ok = 0, tot = 0;
  for (const [name, a, b, lo, hi] of T) for (const [x, y] of [a, b]) {
    const r = await pick(x, y); tot++; const good = r && r.f === H && r.l >= lo && r.l <= hi; if (good) ok++;
    console.log((good ? "OK  " : "BAD ") + name, [x, y].join(","), JSON.stringify(r));
  }
  console.log(`LINUX DESKTOP: ${ok}/${tot} correct`);
  c.ws.close();
})().catch((e) => { console.error("ERR", e.stack || e.message); process.exit(1); });
