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

  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByText("Starting…").first().waitFor();
  ok(true, "the preview starts by itself, no button needed");
  await p.locator('iframe[title="Live preview"]').waitFor({ timeout: 6000 });
  ok(await p.getByText("Running", { exact: true }).isVisible(), "running badge");
  const frame = p.frameLocator('iframe[title="Live preview"]');
  ok(await frame.getByText("Friday Scores").isVisible(), "app is shown in the phone frame");
  await p.screenshot({ path: `${S}/31-preview-running.png` });

  // Klikken in het canvas kiest een element voor de chat en bedient de app niet
  ok(
    await p.getByTestId("pick-banner").getByText("Whole page: HomeScreen.").isVisible(),
    "banner says the whole page is selected",
  );
  const layer = p.getByTestId("pick-layer");
  const lb = await layer.boundingBox();
  // een gewone klik (of te klein vak) kiest niets en bedient de app niet
  await p.mouse.click(lb.x + lb.width * 0.5, lb.y + lb.height * 0.4);
  await p.getByTestId("pick-banner").getByText("Drag to draw a box.").waitFor();
  ok((await p.locator("[data-kind=element]").count()) === 0, "a plain click selects nothing");
  // slepen tekent het vak
  await p.mouse.move(lb.x + lb.width * 0.2, lb.y + lb.height * 0.3);
  await p.mouse.down();
  await p.mouse.move(lb.x + lb.width * 0.5, lb.y + lb.height * 0.45, { steps: 5 });
  ok(await p.getByTestId("pick-box").isVisible(), "the box is drawn while dragging");
  await p.mouse.move(lb.x + lb.width * 0.8, lb.y + lb.height * 0.6, { steps: 5 });
  await p.mouse.up();
  await p.getByTestId("pick-banner").getByText("Only inside the box.").waitFor();
  const bb = await p.getByTestId("pick-box").boundingBox();
  ok(
    Math.abs(bb.width - lb.width * 0.6) < lb.width * 0.03 && Math.abs(bb.height - lb.height * 0.3) < lb.height * 0.03,
    "the box covers exactly what was dragged",
  );
  ok(
    (await p.locator("[data-kind=element]").innerText()).includes("Only inside the box on HomeScreen"),
    "chat chip says the AI only touches the box",
  );
  ok(await frame.getByText("7", { exact: true }).isVisible(), "the click did not reach the app");
  await p.screenshot({ path: `${S}/35-element-selected.png` });
  await p.getByRole("button", { name: "Select whole page" }).click();
  await p.locator('[data-kind="page"]').waitFor();
  ok(true, "one click goes back to the whole page");
  await p.getByRole("button", { name: "Refresh", exact: true }).click();
  await p.locator('iframe[title="Live preview"]').waitFor();
  ok(true, "refresh keeps the preview");
  await p.getByRole("radio", { name: "Small phone" }).click();
  ok(await p.locator('iframe[title="Live preview"]').isVisible(), "device switch keeps preview");
  await p.getByRole("button", { name: "Stop", exact: true }).click();
  await p.getByText("See your app live").waitFor();
  ok(true, "stop returns to empty state");

  // build error -> banner -> fix with Claude
  await p.goto("http://localhost:1420/?mock=ready#/project/water_tracker");
  const alert = p.getByRole("alert").filter({ hasText: "The app has a problem" });
  await alert.waitFor({ timeout: 6000 });
  ok(await alert.getByText("lib/main.dart line 12").isVisible(), "problem shows file and line");
  await p.screenshot({ path: `${S}/32-preview-error.png` });
  await alert.getByRole("button", { name: "Let Claude fix this" }).click();
  await p.getByText("Claude is working…").waitFor();
  ok(await p.getByText("Expected ';' after this.").first().isVisible(), "error text was sent to the chat");
  ok(
    await alert.getByRole("button", { name: "Let Claude fix this" }).isDisabled(),
    "fix button disabled while Claude works",
  );
  await p.getByText("Claude is working…").waitFor({ state: "detached", timeout: 15000 });
  await p.getByRole("button", { name: "Show log" }).click();
  await p.screenshot({ path: `${S}/34-log.png` });
  ok(await p.getByLabel("Technical log").isVisible(), "log can be shown");

  // real device dialog
  await p.getByRole("button", { name: "Real phone" }).click();
  await p.getByText("Pixel 8").waitFor();
  ok(await p.getByText("How to connect an Android phone").isVisible(), "steps visible");
  await p.screenshot({ path: `${S}/33-devices.png` });
  await p.getByRole("button", { name: "Run here" }).first().click();
  await p.getByText("Running on 1A2B3C").waitFor({ timeout: 6000 });
  ok(true, "running on a real device shows device state, no iframe");
  ok((await p.locator("iframe").count()) === 0, "no iframe for device");

  await p.goto("http://localhost:1420/?mock=nodevices#/project/friday_scores");
  await p.getByRole("button", { name: "Real phone" }).click();
  await p.getByText("No phone found yet").waitFor();
  ok(true, "empty device list explains what to do");

  await p.goto("http://localhost:1420/?mock=ready#/settings");
  await p.getByRole("radio", { name: "Nederlands" }).click();
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByText("Hele pagina: HomeScreen").waitFor();
  ok(true, "preview translated");
  ok(errs.filter((e) => !/create project failed/.test(e)).length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
