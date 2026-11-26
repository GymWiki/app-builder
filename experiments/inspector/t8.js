const { chromium } = require("playwright");
const { connect } = require("./vm.js");
const fs = require("fs");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const short = (f) => (f || "").replace(/^file:\/\/\/tmp\/exp\/probe\//, "");
const F = "/tmp/exp/probe/lib/home_screen.dart";
(async () => {
  const c = await connect();
  const br = await chromium.connectOverCDP("http://127.0.0.1:9333");
  const page = br.contexts()[0].pages().find(p => p.url().includes("8123"));
  await c.ext("addPubRootDirectories", { arg0: "/tmp/exp/probe" });
  async function pickAt(x, y) {
    await c.ext("show", { enabled: "true" }); await sleep(200);
    await page.mouse.click(x, y); await sleep(400);
    const g = "g" + Math.random();
    const ss = await c.ext("getSelectedSummaryWidget", { previousSelectionId: "", objectGroup: g });
    const tree = await c.ext("getRootWidgetSummaryTree", { objectGroup: g });
    const path = []; const find = (n, p) => { const q = [...p, n]; if (n.valueId === ss.result?.valueId) { path.push(...q); return true; } return (n.children || []).some((ch) => find(ch, q)); };
    find(tree.result, []);
    await c.ext("show", { enabled: "false" });
    const user = path.filter((n) => n.createdByLocalProject).reverse();
    const site = user.find((n) => !/\/lib\/core\//.test(n.creationLocation.file));
    return site ? `${site.description}@${short(site.creationLocation.file)}:${site.creationLocation.line}` : "none";
  }
  const log = (m) => console.log(m);
  log("baseline  'Open list' → " + await pickAt(60, 180));
  // 1) hot reload after inserting 3 lines at the top of the file (all line numbers shift by 3)
  fs.writeFileSync(F, "// shifted 1\n// shifted 2\n// shifted 3\n" + fs.readFileSync(F, "utf8"));
  await new Promise((r) => { require("fs").appendFileSync("/tmp/exp/run.fifo", "r\n"); setTimeout(r, 6000); });
  log("hot reload (+3 lines)  → " + await pickAt(60, 180) + "   (expect line 25)");
  // 2) insert a new widget above and reload: Welcome back text shifts the button down; positions change
  let t = fs.readFileSync(F, "utf8");
  t = t.replace("const SizedBox(height: 24),\n            PrimaryButton(\n              label: 'Open list'", "const SizedBox(height: 24),\n            const Text('Banner line'),\n            const SizedBox(height: 8),\n            PrimaryButton(\n              label: 'Open list'");
  fs.writeFileSync(F, t);
  fs.appendFileSync("/tmp/exp/run.fifo", "r\n"); await sleep(6000);
  const lines = t.split("\n"); const li = lines.findIndex((l) => l.includes("label: 'Open list'")) ;
  const ptxt = lines.findIndex((l, i) => i < li && l.includes("PrimaryButton(")) + 1;
  log(`hot reload (new widget above)  → pick old spot (60,180): ${await pickAt(60, 180)}  | the 'Banner line' text now sits there; new button spot (60,222): ${await pickAt(60, 222)}   (PrimaryButton now at line ${ptxt})`);
  // 3) hot restart
  fs.appendFileSync("/tmp/exp/run.fifo", "R\n"); await sleep(9000);
  log("hot restart  (60,222) → " + await pickAt(60, 222) + `   (expect line ${ptxt})`);
  // restore
  fs.copyFileSync("/tmp/exp/home.orig", F); fs.appendFileSync("/tmp/exp/run.fifo", "R\n"); await sleep(8000);
  log("restored (60,180) → " + await pickAt(60, 180));
  c.ws.close(); await br.close().catch(()=>{});
})().catch(e => { console.error("ERR", e.stack || e.message); process.exit(1); });
