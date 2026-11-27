// Updates: bij elke start wordt gecontroleerd; is er een nieuwere versie dan vraagt Studio om te updaten.
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

  await p.goto("http://localhost:1420/?mock=update#/");
  const dlg = p.getByRole("dialog");
  await dlg.getByText("Studio 0.2.0 is available").waitFor();
  ok(await dlg.getByText("You have version 0.1.0").isVisible(), "the dialog asks to update and names both versions");
  ok(await dlg.getByText("Faster preview and a calmer chat.").isVisible(), "release notes are shown");
  await p.screenshot({ path: `${S}/60-update.png` });
  await dlg.getByRole("button", { name: "Later" }).click();
  ok((await p.getByRole("dialog").count()) === 0, "Later closes it for this session");
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=update#/");
  await p.getByRole("dialog").getByText("Studio 0.2.0 is available").waitFor();
  ok(true, "it asks again at the next start");
  await p.getByRole("button", { name: "Update and restart" }).click();
  await p.getByText("Downloading and installing…").waitFor();
  ok(true, "update shows progress");

  // Geen update: geen dialoog
  await p.goto("about:blank");
  await p.goto("http://localhost:1420/?mock=ready#/");
  await p.waitForTimeout(800);
  ok((await p.getByText(/is available/).count()) === 0, "no dialog when Studio is up to date");
  ok(errs.length === 0, "no errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
