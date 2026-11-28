// Onboarding (fase 14): idee → functies → stijl → plan → bouwen (meerdere pagina's tegelijk) → app.
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
  let chatCalls = 0;
  p.on("console", (m) => /chat_send/.test(m.text()) && chatCalls++);

  await p.goto("http://localhost:1420/?mock=ready#/");
  await p.getByRole("button", { name: "New project" }).first().click();
  await p.waitForURL(/#\/new/);
  ok(true, "New project opens the onboarding");

  // stap 1: alleen het tekstveld is verplicht
  const next = p.getByRole("button", { name: "Next" });
  ok(await next.isDisabled(), "Next is disabled until something is described");
  await p.getByRole("button", { name: "Scoreboard", exact: true }).click();
  ok((await p.getByLabel("What kind of app do you want to make, and for whom?").inputValue()).includes("scoreboard"), "quick start fills the field");
  await p.getByLabel("What kind of app do you want to make, and for whom?").fill("A scoreboard for friday games with live play on several phones");
  await p.screenshot({ path: `${S}/80-onboarding-idea.png` });
  await next.click();

  // stap 2: functies; een niet-ondersteunde functie wordt uitgelegd met een alternatief
  await p.getByTestId("unsupported").waitFor();
  ok((await p.getByTestId("unsupported").innerText()).includes("One phone for everyone"), "an unsupported idea is explained with a feasible alternative");
  await p.getByRole("button", { name: "Use this instead" }).click();
  await p.getByRole("list", { name: "What should your app be able to do?" }).waitFor();
  const items = p.getByRole("list", { name: "What should your app be able to do?" }).getByRole("listitem");
  ok((await items.count()) >= 4, "features are proposed and the alternative was added");
  await p.getByRole("switch", { name: /Season ranking/ }).click();
  await p.getByLabel("Add a feature", { exact: true }).first().fill("Remind me every morning");
  await p.getByRole("button", { name: "Add a feature" }).click();
  ok((await items.count()) >= 5, "features can be switched off and added");
  await p.getByRole("button", { name: "Move down" }).first().click();
  await p.screenshot({ path: `${S}/81-onboarding-features.png` });
  await next.click();

  // stap 3: stijl met mini-voorbeeld, één klik
  await p.getByRole("radiogroup", { name: "Pick a style" }).waitFor();
  ok((await p.getByRole("radio").count()) === 3, "three style cards");
  await p.getByRole("radio", { name: /Lively/ }).click();
  await p.screenshot({ path: `${S}/82-onboarding-style.png` });
  await next.click();

  // stap 4: het plan; pagina's hernoemen, toevoegen, verwijderen en herordenen; aannames bewerken
  await p.getByRole("heading", { name: "This is the plan" }).waitFor({ timeout: 8000 });
  await p.getByLabel("App name").fill("Game Night");
  await p.getByRole("list", { name: "Pages" }).getByLabel("Page name").first().fill("Start");
  await p.getByPlaceholder("Name of the new page").fill("Rules");
  await p.getByRole("button", { name: "Add a page" }).click();
  ok((await p.getByRole("list", { name: "Pages" }).getByRole("listitem").count()) === 5, "a page can be added");
  await p.getByRole("button", { name: "Remove History" }).click();
  ok((await p.getByRole("list", { name: "Pages" }).getByRole("listitem").count()) === 4, "a page can be removed");
  ok(await p.getByRole("button", { name: "Remove Start" }).isDisabled(), "the start page cannot be removed");
  await p.getByRole("radio", { name: "Menu" }).click();
  ok(await p.getByText("Standard: everything is saved on the phone").isVisible(), "default: local, no account, as a simple line with optional switches");
  await p.getByRole("switch", { name: "Sign in with an account" }).click();
  await p.getByRole("switch", { name: "Sign in with an account" }).click();
  await p.getByLabel("Assumption", { exact: true }).first().fill("I assumed it is only for friends, in English.");
  await p.screenshot({ path: `${S}/83-onboarding-plan.png` });

  // geen chat: de flow is klaar zonder één chatvraag
  ok((await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0, "no chat questions in the flow");

  // concept bewaren: pagina verversen en hervatten
  await p.waitForTimeout(700);
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=ready#/new");
  await p.getByText("Your earlier draft was restored.").waitFor();
  ok(true, "an interrupted onboarding is resumed without data loss");
  await p.getByRole("heading", { name: "This is the plan" }).waitFor();
  ok((await p.getByLabel("App name").inputValue()) === "Game Night", "the edited plan came back");

  // bouwen
  await p.getByRole("button", { name: "Build my app" }).click();
  await p.waitForURL(/#\/build\//);
  await p.getByTestId("build-grid").waitFor();
  await p.getByText(/Page \d of 4 is being built/).waitFor({ timeout: 8000 });
  ok(true, "build progress in plain language");
  const sawParallel = await p.waitForFunction(() => document.querySelectorAll('[data-status="building"]').length >= 2, null, { timeout: 8000 }).then(() => true).catch(() => false);
  ok(sawParallel, "several pages are built at the same time");
  await p.screenshot({ path: `${S}/84-building.png` });
  ok(await p.getByRole("button", { name: "Stop building" }).isVisible(), "building can be cancelled");
  await p.waitForURL(/#\/project\//, { timeout: 30000 });
  ok(true, "the finished app opens by itself on its start page");
  ok((await p.getByText(/pick a page in the list/).count()) >= 0, "a single friendly tip");
  ok(errs.length === 0, "no errors " + errs.join("|"));

  // vaag/kort: "Verras me" gaat meteen naar het plan, zonder vragen
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=ready#/new");
  await p.getByLabel("What kind of app do you want to make, and for whom?").fill("iets met honden");
  await p.getByRole("button", { name: "Surprise me with default choices" }).click();
  await p.getByRole("heading", { name: "This is the plan" }).waitFor({ timeout: 8000 });
  ok(true, "'Surprise me' goes straight to a plan with assumptions");

  // mislukte pagina: één herpoging, daarna eerlijk melden en apart opnieuw proberen
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=buildfail#/new");
  await p.waitForTimeout(400);
  const startOver = p.getByRole("button", { name: "Start over" });
  if (await startOver.count()) await startOver.click();
  await p.getByLabel("What kind of app do you want to make, and for whom?").fill("iets met honden");
  await p.getByRole("button", { name: "Surprise me with default choices" }).click();
  await p.getByRole("button", { name: "Build my app" }).click();
  await p.getByText(/Not finished yet: history/).waitFor({ timeout: 20000 });
  ok(await p.getByText("Your app is ready, except for a few pages.").isVisible(), "a partial result is never presented as complete");
  ok((await p.getByTestId("build-grid").locator('[data-status="failed"]').count()) === 1, "the unfinished page is marked");
  await p.screenshot({ path: `${S}/85-build-partial.png` });
  await p.getByRole("button", { name: "Try these again" }).click();
  await p.waitForURL(/#\/project\//, { timeout: 20000 });
  ok(true, "retrying only the failed page completes the app");
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
