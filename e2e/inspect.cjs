// Inspect-modus: het scherm komt als beeldstroom, een getekend vak wordt omgezet in widgets (nagebootste inspector).
const { chromium } = require("playwright");
const S = process.argv[2];
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 }, locale: "en-US" })).newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  await p.goto("http://localhost:1420/?mock=inspect#/project/friday_scores");
  await p.getByTestId("pick-layer").waitFor({ timeout: 8000 });
  ok((await p.locator("iframe").count()) === 0, "no iframe in inspect mode");
  await p.locator('img[src^="data:image/svg+xml"]').waitFor({ timeout: 5000 });
  ok(true, "the screen arrives as a stream of images");
  const lb = await p.getByTestId("pick-layer").boundingBox();
  // vak om de knop onderaan het getal
  await p.mouse.move(lb.x + lb.width * 0.35, lb.y + lb.height * 0.34);
  await p.mouse.down();
  await p.mouse.move(lb.x + lb.width * 0.65, lb.y + lb.height * 0.4, { steps: 4 });
  await p.mouse.up();
  await p.getByTestId("pick-banner").getByText("Button “+1” on Home.").waitFor();
  ok(true, "the box becomes a named element: Button “+1” on Home");
  ok(
    (await p.locator("[data-kind=element]").innerText()).includes("Button “+1” on Home"),
    "chat chip names the element, not just a box",
  );
  await p.screenshot({ path: `${S}/36-inspect-selected.png` });
  // een ander vak bovenin: de titel
  await p.getByRole("button", { name: "Select whole page" }).click();
  await p.mouse.move(lb.x + lb.width * 0.05, lb.y + lb.height * 0.02);
  await p.mouse.down();
  await p.mouse.move(lb.x + lb.width * 0.5, lb.y + lb.height * 0.07, { steps: 4 });
  await p.mouse.up();
  await p.getByTestId("pick-banner").getByText("Text “Friday Scores” on Home.").waitFor();
  ok(true, "a box at the top selects the title text");
  ok(errs.length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
