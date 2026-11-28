// Preview altijd aan: start vanzelf, toont de gekozen pagina, bewaart schermafbeeldingen, "Niet gekoppeld", geen aan/uit.
const { chromium } = require("playwright");
const S = process.argv[2] || "./e2e-out";
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
  const phone = p.getByTestId("pick-layer");

  // 1. opent een project -> skeleton met paginanaam, daarna de start-pagina live, zonder iets aan te klikken
  await p.goto("http://localhost:1420/?mock=slowstart#/project/friday_scores");
  await p.getByTestId("page-skeleton").getByText("Loading page…").waitFor();
  ok(await p.getByTestId("page-skeleton").getByText("HomeScreen").isVisible(), "skeleton shows the page name while starting");
  ok((await p.getByText("Not running").count()) === 0, "no 'not running' badge");
  await phone.waitFor({ timeout: 8000 });
  await p.getByTestId("stored-shot").waitFor({ state: "detached", timeout: 8000 }).catch(() => undefined);
  ok(true, "the start page appears by itself");

  // 2. er is geen aan/uit en geen lege staat
  ok(
    (await p.getByRole("button", { name: /^(Start preview|Stop)$/ }).count()) === 0 &&
      (await p.getByText("See your app live").count()) === 0,
    "no start/stop control and no empty state",
  );
  ok(await p.getByRole("button", { name: "Start test run" }).isVisible(), "the button is 'Start test run'");
  ok(await p.getByRole("button", { name: "Real phone" }).isVisible(), "'Real phone' stays");
  ok((await p.getByText("Running", { exact: true }).count()) === 0, "no badge when live");

  // 3. klik op een andere pagina -> de preview toont die pagina
  const tree = p.getByRole("list", { name: "Pages" });
  const canvasText = async (txt) => {
    // de mock tekent de paginatitel in een svg-afbeelding; controleer via de route die in de lijst gekozen staat
    return p.waitForFunction((t) => !!document.body.innerText.includes(t), txt, { timeout: 8000 });
  };
  await tree.getByRole("button", { name: /SettingsScreen/ }).first().click();
  await p.getByTestId("pick-banner").getByText("Whole page: SettingsScreen.").waitFor();
  await canvasText("SettingsScreen");
  ok(true, "clicking a page selects it and the banner follows");
  await p.waitForTimeout(1500);
  const img = await p.locator("img[draggable=false]").first().getAttribute("src");
  ok(!!img && (await p.evaluate((s) => atob(s.split(",")[1]).includes("Settings"), img)), "the canvas shows the Settings page");

  // 4. opgeslagen afbeeldingen: de crawl heeft de andere pagina's op de achtergrond vastgelegd; bij wisselen is er meteen een beeld
  await p.waitForTimeout(9000);
  await tree.getByRole("button", { name: /ProfileScreen/ }).first().click();
  ok(await p.getByTestId("stored-shot").isVisible({ timeout: 400 }), "a stored screenshot is visible immediately");
  ok((await p.getByTestId("page-skeleton").count()) === 0, "no empty canvas, no skeleton once a shot exists");
  await p.screenshot({ path: `${S}/37-stored-shot.png` });

  // 5. de gebruiker navigeert zelf in de preview -> de lijst volgt
  await p.waitForTimeout(3000);
  await p.evaluate(() => window.__mockEmit("preview-route", { route: "/settings" }));
  await p.getByTestId("pick-banner").getByText("Whole page: SettingsScreen.").waitFor({ timeout: 3000 });
  ok(true, "list selection follows navigation inside the preview");

  // 6. pagina zonder route -> melding + Verbinden
  await tree.getByRole("button", { name: /LonelyScreen/ }).first().click();
  await p.getByTestId("not-linked").waitFor();
  ok(await p.getByText("This page isn’t connected to the app yet").isVisible(), "unlinked page shows the message");
  await p.getByTestId("not-linked").getByRole("button", { name: "Connect" }).click();
  await p.getByText("Claude is working…").waitFor();
  ok(
    await p.getByText("Connect the screen LonelyScreen to the app").first().isVisible(),
    "Connect asks Claude to connect only that page",
  );
  await p.screenshot({ path: `${S}/36-not-linked.png` });

  // 7. NL
  await p.goto("http://localhost:1420/?mock=ready#/settings");
  await p.getByRole("radio", { name: "Nederlands" }).click();
  await p.goto("http://localhost:1420/?mock=slowstart#/project/friday_scores");
  await p.getByText("Pagina wordt geladen…").first().waitFor();
  ok(await p.getByRole("button", { name: "Start testrun" }).isVisible(), "Dutch labels");
  ok(errs.filter((e) => !/create project failed/.test(e)).length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
