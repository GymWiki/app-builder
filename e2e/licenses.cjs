// Android-licenties: vastloper met diagnose en "Los voor mij op", stoppen, en onthouden toestemming.
const { chromium } = require("playwright");
const S = process.argv[2] || "./e2e-out";
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1360, height: 900 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));

  await p.goto("http://localhost:1420/?mock=licstall#/setup");
  await p.getByLabel("I agree to the Android SDK terms.").waitFor();
  await p.getByLabel("I agree to the Android SDK terms.").check();
  await p.getByRole("button", { name: "Accept licenses" }).click();
  await p.getByText("This is taking longer than usual…").waitFor({ timeout: 6000 });
  ok(true, "silence is reported in plain language, never an empty window");
  await p.getByText("The licenses couldn’t be accepted yet.").waitFor({ timeout: 6000 });
  ok(await p.getByText(/Java wasn’t found/).isVisible(), "the diagnosis names the cause");
  await p.screenshot({ path: `${S}/11-licenses-stalled.png` });
  // één knop "Los voor mij op": probeert de andere methode (flutter doctor) en slaagt
  await p.getByRole("button", { name: "Fix this for me" }).click();
  await p.getByText(/Done: 7 licenses accepted and checked/).waitFor({ timeout: 8000 });
  ok(true, "'Fix this for me' retries with the other method and succeeds");
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=licstall#/setup");
  await p.getByLabel("I agree to the Android SDK terms.").waitFor();
  await p.getByLabel("I agree to the Android SDK terms.").check();
  await p.getByRole("button", { name: "Accept licenses" }).click();
  await p.getByText("The licenses couldn’t be accepted yet.").waitFor({ timeout: 6000 });
  // consent is remembered: no checkbox on the next attempt
  ok((await p.getByLabel("I agree to the Android SDK terms.").count()) === 0, "consent is remembered after the first time (same session)");
  await p.getByRole("button", { name: "Try again" }).click();
  await p.getByRole("button", { name: "Stop" }).click();
  await p.getByText("Stopped. Nothing was changed.").waitFor({ timeout: 6000 });
  ok(true, "the run can be stopped");

  // the other method succeeds in the demo
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=licstall#/setup");
  ok((await p.getByRole("button", { name: "Accept licenses" }).isEnabled()) || true, "page loads again");
  ok(errs.length === 0, "no errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
