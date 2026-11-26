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
  let cc = c;
  const reconnect = async () => { try { cc.ws.close(); } catch {} cc = await connect(); await cc.ext("addPubRootDirectories", { arg0: "/tmp/exp/probe" }); };
  fs.writeFileSync(F, "// shifted 1\n// shifted 2\n" + fs.readFileSync(F, "utf8"));
  fs.appendFileSync("/tmp/exp/run.fifo", "R\n"); await sleep(9000);
  await reconnect();
  const ext = cc.ext; c.ext = ext;
  log("hot restart (+2 lines) after reconnect → " + await pickAt(60, 180) + "   (expect line 24)");
  fs.copyFileSync("/tmp/exp/home.orig", F); fs.appendFileSync("/tmp/exp/run.fifo", "R\n"); await sleep(8000);
  await reconnect(); c.ext = cc.ext;
  log("restored → " + await pickAt(60, 180) + "   (expect line 22)");
  cc.ws.close(); await br.close().catch(()=>{});
})().catch(e => { console.error("ERR", e.stack || e.message); process.exit(1); });
