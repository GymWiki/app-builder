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
  // Android-licenties: geen terminal. Zonder toestemming kan er niet gestart worden; met het vinkje loopt het vanzelf.
  const accept = p.getByRole("button", { name: "Accept licenses" });
  ok(await accept.isDisabled(), "accepting licenses needs the consent checkbox first");
  ok(await p.getByRole("link", { name: /Read the terms/ }).count() === 0 && (await p.getByRole("button", { name: /Read the terms/ }).count()) === 1, "a link to the Android SDK terms is shown");
  await p.getByLabel("I agree to the Android SDK terms.").check();
  await accept.click();
  await p.getByText(/License \d of 7 accepted/).first().waitFor({ timeout: 6000 });
  ok(true, "progress is shown in plain language");
  ok((await p.locator("[role=group][aria-label=Terminal]").count()) === 0, "no terminal for the licenses");
  await p.getByText(/Done: 7 licenses accepted and checked/).waitFor({ timeout: 8000 });
  ok(true, "the check (flutter doctor) runs automatically; no 'Check again' needed");
  await p.screenshot({ path: `${S}/10-setup-licenses.png` });
  // Claude installeren (winget, zonder terminal) en daarna inloggen
  await p.getByRole("button", { name: "Install for me" }).click();
  await p.getByRole("alertdialog").getByText(/--accept-package-agreements/).waitFor();
  ok(true, "the confirm dialog shows the exact command incl. the agreement flags");
  await p.getByRole("button", { name: "Run command" }).click();
  await p.getByText("Installed").first().waitFor({ timeout: 6000 });
  ok((await p.locator("[role=group][aria-label=Terminal]").count()) === 0, "installing needs no terminal either");
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
  await p.getByRole("button", { name: "Hide pages" }).click();
  await p.waitForTimeout(400);
  const treePanel = p.locator("[data-panel][id$='tree'], #tree").first();
  const w = await treePanel.evaluate((el) => el.getBoundingClientRect().width);
  ok(w < 2, "pages tree collapses (panel width " + w + ")");
  ok(await p.getByRole("button", { name: "Show pages" }).isVisible(), "toggle label flips to Show pages");
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
    .locator("[data-panel][id$='tree'], #tree")
    .first()
    .evaluate((el) => el.getBoundingClientRect().width);
  ok(w2 < 2, "collapsed layout restored after reload (width " + w2 + ")");
  await p.getByRole("button", { name: "Hide menu" }).click();
  ok(!(await p.getByRole("navigation", { name: "Project menu" }).isVisible()), "project menu can be hidden");
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
