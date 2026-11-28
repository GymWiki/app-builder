// "Hele app" en mappen als bereik: selectie, chip, raster, brede opdracht in stappen, terugdraaien.
const { chromium } = require("playwright");
const S = process.argv[2] || "./e2e-out";
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1500, height: 900 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  const tree = p.getByRole("list", { name: "Pages" });
  await tree.waitFor();

  // standaard: gedrag zoals nu (huidige pagina), "Hele app" alleen als de gebruiker dat kiest
  ok((await p.getByTestId("scope-app").getAttribute("aria-pressed")) === "false", "by default the current page is the scope, not the whole app");
  await p.getByTestId("scope-app").click();
  const chip = p.locator('[data-kind="scope"]');
  await chip.waitFor();
  ok((await chip.innerText()).includes("Whole app"), "the chat shows a 'Whole app' chip");
  ok(await p.getByTestId("pages-grid").isVisible(), "the canvas shows the grid of all pages");
  ok((await p.getByTestId("pages-grid").getByRole("listitem").count()) >= 5, "every page has a thumbnail slot");
  ok(await p.getByRole("button", { name: "Add a back button to every page" }).isVisible(), "suggestions fit the scope");
  await p.screenshot({ path: `${S}/70-whole-app.png` });

  // een miniatuur kiest alleen die pagina
  await p.getByTestId("pages-grid").getByRole("button", { name: /SettingsScreen/ }).click();
  ok((await p.locator('[data-kind="scope"]').count()) === 0 && (await p.getByTestId("pages-grid").count()) === 0, "clicking a thumbnail goes back to that one page");

  // map als bereik is er alleen met mappen; meerdere pagina's met Ctrl-klik
  await tree.getByRole("button", { name: /HomeScreen/ }).first().click();
  await tree.getByRole("button", { name: /AboutPage/ }).first().click({ modifiers: ["Control"] });
  await tree.getByRole("button", { name: /ProfileScreen/ }).first().click({ modifiers: ["Control"] });
  await p.locator('[data-kind="scope"]').waitFor();
  ok((await p.locator('[data-kind="scope"]').innerText()).includes("3 pages"), "Ctrl-click selects several pages: chip says 3 pages");
  await p.locator('[data-kind="scope"]').getByRole("button").click();
  ok((await p.locator('[data-kind="scope"]').count()) === 0, "the cross goes back to the current page");

  // brede, eenvoudige opdracht → per pagina, met voortgang, samenvatting en terugdraaien
  await p.getByTestId("scope-app").click();
  await p.locator("textarea").first().fill("Make all buttons rounded");
  await p.keyboard.press("Enter");
  const task = p.getByTestId("scope-task");
  await task.waitFor({ timeout: 8000 });
  await task.getByText(/Shared parts first|Page 1 of/).waitFor({ timeout: 8000 });
  ok(true, "a wide simple change shows plain-language progress");
  await task.getByText(/Page \d of \d/).first().waitFor({ timeout: 30000 });
  ok(true, "progress is per page ('Page n of N')");
  await task.getByText(/pages changed/).waitFor({ timeout: 90000 });
  ok(true, "a summary in plain language follows");
  await p.screenshot({ path: `${S}/71-scope-done.png` });
  await task.getByRole("button", { name: "Undo" }).click();
  await task.getByText("Everything was put back as it was.").waitFor({ timeout: 6000 });
  ok(true, "the whole change can be undone with one click");
  ok(errs.length === 0, "no errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
