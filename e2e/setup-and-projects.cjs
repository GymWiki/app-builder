const { chromium } = require("playwright");
const S = process.argv[2];
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1360, height: 860 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));

  // ── wizard: fresh start redirects, install + sign-in flow, finish
  await p.goto("http://localhost:1420/?mock=fresh#/");
  await p.waitForURL(/#\/setup/);
  ok(true, "fresh start redirects to /setup");
  await p
    .getByText("2 of 5 ready")
    .waitFor({ timeout: 6000 })
    .catch(() => {});
  const finishBtn = p.getByRole("button", { name: "Start building" });
  ok(await finishBtn.isDisabled(), "finish disabled while tools missing");
  await p.getByRole("button", { name: "Accept licenses" }).click();
  await p.getByRole("alertdialog").waitFor();
  ok(
    await p.getByRole("alertdialog").getByText("flutter doctor --android-licenses").isVisible(),
    "confirm dialog shows exact command",
  );
  await p.getByRole("button", { name: "Run command" }).click();
  await p.locator("[role=group][aria-label=Terminal]").waitFor();
  ok(true, "terminal opens after confirmation");
  await p.getByText("Finished").first().waitFor({ timeout: 6000 });
  await p.screenshot({ path: `${S}/10-setup-terminal.png` });
  // Claude installeren (mock: winget) en daarna inloggen
  await p.getByRole("button", { name: "Install for me" }).click();
  await p.getByRole("button", { name: "Run command" }).click();
  await p.getByText("Finished").first().waitFor({ timeout: 6000 });
  await p.getByRole("button", { name: "Sign in" }).click();
  await p.getByText("Finished").first().waitFor({ timeout: 6000 });
  await p.getByText("Connected ✓").waitFor({ timeout: 8000 });
  await p.getByText("5 of 5 ready").waitFor({ timeout: 8000 });
  ok(true, 'all rows green after recheck, "Connected ✓" shown');
  await p.getByRole("button", { name: "Start building" }).click();
  await p.waitForURL(/#\/$/);
  ok(true, "finish navigates home");

  // ── create project
  await p.getByRole("button", { name: "New project" }).first().click();
  await p.getByLabel("App name").fill("Fail please");
  await p.getByRole("radio", { name: /Tracker/ }).click();
  await p.getByRole("button", { name: "Create project" }).click();
  await p.getByRole("alert").waitFor();
  const alert = await p.getByRole("alert").innerText();
  ok(
    /Flutter SDK isn’t available yet/.test(alert) && /Open setup/.test(alert),
    "tool_missing shows friendly message + Open setup: " + alert.replace(/\n/g, " | "),
  );
  await p.getByLabel("App name").fill("Garden Log");
  await p.getByRole("button", { name: "Create project" }).click();
  await p.getByText("Creating your project…").first().waitFor();
  await p.screenshot({ path: `${S}/11-creating.png` });
  await p.waitForURL(/#\/project\//, { timeout: 8000 });
  ok(await p.getByRole("heading", { name: "Garden Log" }).isVisible(), "creation navigates into the new project");

  // ── panels: collapse + persistence
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByRole("button", { name: "Hide menu" }).click();
  await p.waitForTimeout(400);
  const nav = p.getByRole("navigation", { name: "Project menu" });
  const w = await nav.evaluate((el) => el.closest("[data-panel]")?.getBoundingClientRect().width ?? -1);
  ok(w < 2, "menu collapses (panel width " + w + ")");
  ok(await p.getByRole("button", { name: "Show menu" }).isVisible(), "toggle label flips to Show menu");
  await p.reload();
  await p.getByRole("heading", { name: "Friday Scores" }).waitFor();
  await p.waitForTimeout(500);
  const stored = await p.evaluate(() =>
    Object.entries(localStorage)
      .filter(([k]) => k.includes("panel") || k.includes("layout"))
      .map(([k, v]) => k + "=" + v)
      .join(" ; "),
  );
  ok(stored.length > 0, "layout stored in localStorage: " + stored.slice(0, 120));
  const w2 = await p
    .getByRole("navigation", { name: "Project menu" })
    .evaluate((el) => el.closest("[data-panel]")?.getBoundingClientRect().width ?? -1);
  ok(w2 < 2, "collapsed layout restored after reload (width " + w2 + ")");
  await p.screenshot({ path: `${S}/12-project-collapsed.png` });

  // ── back + remove with confirmation
  await p.getByRole("link", { name: "Back" }).click();
  await p.getByRole("button", { name: "Project actions for Friday Scores" }).click();
  await p.getByRole("menuitem", { name: /Remove/ }).click();
  const dlg = p.getByRole("dialog");
  ok(
    (await dlg.getByRole("button", { name: "Yes, delete everything" }).count()) === 0,
    "delete needs a second confirmation step",
  );
  await dlg.getByRole("button", { name: "Delete project and files" }).click();
  await dlg.getByRole("button", { name: "Yes, delete everything" }).click();
  await p.getByText("“Friday Scores” and its files were deleted").waitFor();
  ok(true, "project deleted after explicit confirmation");

  // ── search + keyboard focus ring
  await p.getByLabel("Search projects").fill("zzz");
  await p.getByText("No projects match “zzz”").waitFor();
  ok(true, "no-results empty state");

  // ── language switch to NL
  await p.goto("http://localhost:1420/?mock=ready#/settings");
  await p.getByRole("radio", { name: "Nederlands" }).click();
  await p.getByRole("heading", { name: "Instellingen", level: 1 }).waitFor();
  ok(true, "language switch → Dutch");
  await p.screenshot({ path: `${S}/13-settings-nl.png` });

  // ── unknown project
  await p.goto("http://localhost:1420/?mock=ready#/project/nope");
  await p.getByText("We kunnen dit project niet vinden").waitFor();
  ok(true, "unknown project shows friendly page");

  ok(errs.length === 0, "no console/page errors " + errs.join(" || "));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message);
  process.exit(1);
});
