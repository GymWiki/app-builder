const { chromium } = require("playwright");
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByText("Getting Claude ready for this project…").waitFor();
  ok(true, "quiet status line while the profile is applied");
  await p.getByText("Claude is ready for this project ✓").waitFor({ timeout: 5000 });
  ok(true, "status turns to ready");
  await p.getByText("Claude is ready for this project ✓").waitFor({ state: "detached", timeout: 8000 });
  ok(true, "ready line disappears by itself");
  // dev mode is hidden: seven clicks on the version line
  await p.goto("http://localhost:1420/?mock=ready#/settings");
  ok((await p.getByText("Developer mode").count()) === 0, "developer mode hidden by default");
  const line = p.getByText(/^Studio 0\.1\.0 on/);
  for (let i = 0; i < 7; i++) await line.click();
  await p.getByRole("heading", { name: "Developer mode" }).waitFor();
  await p.getByText("Runs: 12").first().waitFor();
  ok(true, "developer panel shows the run metrics");
  ok(await p.getByLabel("Model for the next tasks").isVisible(), "developer mode has a manual model choice");
  ok(
    await p
      .getByText(/haiku: 7 runs, 100% succeeded/)
      .first()
      .isVisible(),
    "results per model are listed",
  );
  ok(
    await p
      .getByText(/complex: 2 runs, 50% succeeded/)
      .first()
      .isVisible(),
    "results per class are listed",
  );
  await p.getByLabel("Model for the next tasks").selectOption("strong");
  await p.waitForTimeout(300);
  ok(
    await p
      .getByText(/small 7, screen 3, shared 1, project 1/)
      .first()
      .isVisible(),
    "levels per run are listed",
  );
  ok(
    await p
      .getByText(/Profile version 1 \(current 1\)/)
      .first()
      .isVisible(),
    "developer panel shows the technical profile state",
  );
  ok(
    await p
      .getByText(/Dart MCP server: Dart 3\.14\.0/)
      .first()
      .isVisible(),
    "dart mcp state shown",
  );
  for (let i = 0; i < 7; i++) await line.click();
  await p.getByRole("heading", { name: "Developer mode" }).waitFor({ state: "detached" });
  ok(true, "seven more clicks turn it off again");
  ok(
    errs.filter((e) => !/create project failed/.test(e)).length === 0,
    "no console errors " + errs.join("|").slice(0, 200),
  );
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
