// UX-bewaking (Fase 13): kernflows mogen een maximum aantal handelingen kosten en nooit een chatbeurt nodig hebben.
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
  let actions = 0; // handelingen = klikken + invoer in deze flow
  const act = async (fn) => {
    actions++;
    await fn();
  };

  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByRole("button", { name: "Theme", exact: true }).first().click();
  // Thema aanpassen: 1 handeling voor de kleur, nul chatbeurten
  await act(() => p.getByRole("radio", { name: "#0D9488" }).click());
  await p.getByText("Color updated. The preview refreshes by itself.").first().waitFor();
  ok(actions <= 1, "changing the color takes one action");
  // lettertype en stijl: ook één handeling, geen chat
  await act(() => p.getByRole("radio", { name: "Lora" }).click());
  await act(() => p.getByRole("radio", { name: "Minimal" }).click());
  ok(actions <= 3, "color + font + look take three actions in total");
  ok(
    (await p.getByRole("button", { name: /Let Claude|Connect the theme|Ask Claude/ }).count()) === 0,
    "no chat/AI button is needed to finish a theme setting",
  );
  ok((await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0, "the chat is not even visible on this page");

  // Publiceren: wat Studio zelf kan oplossen gaat met één klik, zonder chat
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name: /^Publish/ }).click();
  const fixAll = p.getByRole("button", { name: "Fix everything I can" });
  await fixAll.waitFor();
  const before = actions;
  await act(() => fixAll.click());
  await p.getByText(/things? fixed/).first().waitFor();
  ok(actions - before === 1, "fixing everything fillable takes one action");
  ok((await fixAll.count()) === 0, "nothing fixable is left afterwards");
  ok((await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0, "no chat turn needed");

  // Data & inloggen: provider kiezen, methode aanvinken, opzetten = 3 handelingen, zonder chat
  await p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name: /^Data/ }).click();
  const d0 = actions;
  await act(() => p.getByRole("radio", { name: /Firebase/ }).click());
  await act(() => p.getByLabel("Email and password").click());
  await act(() => p.getByRole("button", { name: "Set this up" }).click());
  await p.getByText("Studio set this up itself").waitFor();
  ok(actions - d0 === 3, "connecting a database with login takes three actions");
  ok(await p.getByText("“Delete my account” is built in").isVisible(), "the account-deletion question is answered automatically (asked only once)");
  ok((await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0, "no chat turn needed for data and login");
  ok(await p.getByRole("button", { name: "Design my own tables with Claude" }).isVisible(), "the AI is offered only for custom tables, as an option");

  // Achtergrondtaak: wat toch de AI nodig heeft, loopt met voortgang en uitkomst, zonder dat de chat erbij komt kijken
  await p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name: /^Assets/ }).click();
  // Splash-scherm: één handeling (standaard is de themakleur en het app-icoon), zonder chat of AI
  const s0 = actions;
  await act(() => p.getByRole("button", { name: "Apply splash screen" }).click());
  await p.getByText(/files? created/).last().waitFor();
  ok(actions - s0 === 1, "a splash screen takes one action");
  await p.getByRole("button", { name: "Use my images" }).click();
  const tray = p.getByTestId("task-tray");
  await tray.waitFor();
  ok(await tray.getByText("Use my images").isVisible(), "the task shows what it is doing, outside the chat");
  ok(await tray.getByRole("button", { name: /Stop|Dismiss/ }).first().isVisible(), "a running task can be stopped");
  await tray.getByText(/Done\./).waitFor({ timeout: 20000 });
  ok(true, "the task ends with a plain-language result");
  ok((await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0, "the chat was not needed to follow it");
  await tray.getByRole("button", { name: "Dismiss" }).click();

  // Meting: in de ontwikkelaarsmodus staat dat Studio het zelf deed, zonder AI
  await p.goto("http://localhost:1420/?mock=ready#/settings");
  const ver = p.getByText(/^Studio \d/).first();
  for (let i = 0; i < 7; i++) await ver.click();
  const ov = p.getByTestId("ux-overview");
  await ov.waitFor();
  const text = await ov.innerText();
  ok(/theme\s+3\s+0\s+0/.test(text.replace(/\n/g, " ").replace(/\s+/g, " ")), "overview: theme = 3 done by Studio, 0 AI calls, 0 chat turns: " + text.replace(/\n/g, " "));
  ok(/Estimated tokens saved: 104,000/.test(text), "token saving is estimated");
  await p.screenshot({ path: `${S}/52-ux-overview.png` });
  ok(errs.length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
