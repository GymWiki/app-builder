// Instellingen → AI-verbindingen: staat per AI, de uitleg over waar je code heen gaat, koppelen met bevestiging, volgorde en limiet.
const { chromium } = require("playwright");
const S = process.argv[2];
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const p = await (await b.newContext({ viewport: { width: 1200, height: 1100 }, locale: "en-US" })).newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  await p.goto("http://localhost:1420/?mock=limit#/settings");
  const claude = p.getByTestId("engine-claude-code");
  const codex = p.getByTestId("engine-codex");
  const gemini = p.getByTestId("engine-gemini");
  await claude.waitFor();
  ok(await claude.getByText(/Limit reached, back around/).isVisible(), "a limit shows when the AI is back");
  ok(await claude.getByText("Used first").isVisible(), "the first one in line is marked");
  ok(
    (await codex.getByText("Not logged in").isVisible()) && (await gemini.getByText("Not installed").isVisible()),
    "install and login state per AI",
  );
  ok(await codex.getByText(/your app’s code is sent to OpenAI/).isVisible(), "one plain sentence: where the code goes");
  ok(
    (await codex.getByText(/Newer in Studio/).isVisible()) && (await claude.getByText(/Newer in Studio/).count()) === 0,
    "less-proven AIs say they get extra checks",
  );
  ok(await gemini.getByText(/npm install -g @google\/gemini-cli/).isVisible(), "how to install is shown");
  // koppelen met bevestiging
  await codex.getByLabel("Use this AI").click();
  const dlg = p.getByRole("alertdialog");
  ok(
    await dlg.getByText(/your app’s code is sent to OpenAI/).isVisible(),
    "linking asks for confirmation with the same sentence",
  );
  await dlg.getByRole("button", { name: "Link" }).click();
  await codex.getByText("Next in line").waitFor();
  ok(true, "the linked AI joins the order");
  await p.getByRole("button", { name: "Move Codex CLI up" }).click();
  await codex.getByText("Used first").waitFor();
  ok(await claude.getByText("Next in line").isVisible(), "the order can be changed");
  await claude.getByRole("button", { name: "Try again now" }).click();
  await claude.getByText("Ready", { exact: true }).waitFor();
  ok(true, "'Try again now' clears the limit");
  await codex.getByRole("button", { name: "Log in" }).click();
  await p.getByText("Log in to Codex CLI").waitFor();
  ok(true, "logging in opens the AI's own terminal, Studio never sees the login");
  await p.screenshot({ path: `${S}/80-engines.png`, fullPage: true });
  await codex.getByLabel("Use this AI").click();
  await codex.getByText("Used first").waitFor({ state: "detached" });
  ok(true, "an AI can be taken out of the list again");
  ok(errs.length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
