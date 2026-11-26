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
  p.on("console", (m) => m.type() === "error" && !/create project failed/.test(m.text()) && errs.push(m.text()));
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByText("What should we build?").waitFor();
  await p.screenshot({ path: `${S}/20-chat-empty.png` });
  ok(true, "empty chat shows suggestions");

  // select a screen -> context chip
  await p.locator("[data-kind=page]").waitFor();
  ok(true, "the start page is active by itself and shown as a context chip");

  // suggestion fills draft, send
  await p.getByRole("button", { name: "Add a second screen with a settings page" }).click();
  const box = p.getByRole("textbox", { name: "Chat with Claude" });
  ok((await box.inputValue()).includes("settings page"), "suggestion fills the draft");
  await box.press("Enter");
  await p.getByText("Claude is working…").waitFor();
  await p
    .getByText("Changed lib/settings_screen.dart")
    .waitFor({ timeout: 8000 })
    .catch(() => {});
  await p.screenshot({ path: `${S}/21-chat-working.png` });
  ok(
    await p.getByRole("region", { name: "Chat with Claude" }).getByRole("button", { name: "Stop" }).isVisible(),
    "stop button while working",
  );
  ok(
    await box.evaluate((el) => el.closest("div").querySelector("button")?.disabled ?? true),
    "send disabled while running",
  );
  await p.getByText("Saved a snapshot").waitFor({ timeout: 10000 });
  await p.getByText("Claude is working…").waitFor({ state: "detached" });
  ok(await p.getByText("Changed lib/settings_screen.dart").isVisible(), "file change card shown in plain language");
  ok(await p.getByText("Ran flutter analyze").isVisible(), "command card shown");
  ok(await p.getByText("Working on Home").isVisible(), "scope shown as a plain sentence for the chosen screen");
  ok(await p.getByText("Everything still works.").isVisible(), "check result shown as a plain sentence");
  ok(
    (await p.getByText(/trivial|tier|haiku|sonnet|opus/i).count()) === 0,
    "no model, class or tier talk in the normal interface",
  );
  ok(
    !(await p
      .getByText(/small|breid_bereik_uit/)
      .first()
      .isVisible()
      .catch(() => false)),
    "no technical level names for normal users",
  );
  ok((await p.locator("strong", { hasText: "settings screen" }).count()) === 1, "markdown bold rendered");
  await p.screenshot({ path: `${S}/22-chat-done.png` });

  // history survives remount
  await p.goto("http://localhost:1420/?mock=ready#/");
  await p.getByRole("link", { name: "Friday Scores" }).first().click();
  await p.getByText("Saved a snapshot").waitFor();
  ok(true, "history reloaded after leaving and returning");

  // permission flow
  const box2 = p.getByRole("textbox", { name: "Chat with Claude" });
  await box2.fill("look up a package on the internet");
  await box2.press("Enter");
  const perm = p.getByRole("alert").filter({ hasText: "wants to use the internet" });
  await perm.waitFor({ timeout: 8000 });
  ok(await perm.getByText("pub.dev").isVisible(), "permission card shows what will be opened");
  await p.screenshot({ path: `${S}/23-chat-permission.png` });
  await perm.getByRole("button", { name: "Allow once" }).click();
  await p.getByText("Opened https://pub.dev/packages/shared_preferences").waitFor({ timeout: 8000 });
  ok(true, "allowing continues the turn");
  await p.getByText("Claude is working…").waitFor({ state: "detached", timeout: 15000 });

  // deny + stop
  await box2.fill("use the internet again");
  await box2.press("Enter");
  const perm2 = p.getByRole("alert").filter({ hasText: "wants to use the internet" });
  await perm2.waitFor({ timeout: 8000 });
  await perm2.getByRole("button", { name: "Don’t allow" }).click();
  await p.getByText("Claude is working…").waitFor({ state: "detached", timeout: 15000 });
  ok(true, "deny lets the turn finish");

  // not logged in error
  await box2.fill("login problem");
  await box2.press("Enter");
  const err = p.getByRole("alert").filter({ hasText: "not signed in to Claude" });
  await err.waitFor({ timeout: 8000 });
  ok(await err.getByRole("link", { name: "Open setup" }).isVisible(), "login error offers Open setup");
  await p.screenshot({ path: `${S}/24-chat-error.png` });

  // new conversation clears; advanced tab
  await p.getByRole("button", { name: "New conversation" }).click();
  await p.getByText("What should we build?").waitFor();
  ok(true, "new conversation starts empty");
  await p.getByRole("tab", { name: "Advanced" }).click();
  await p.getByRole("button", { name: "Start terminal" }).click();
  await p.locator('[role=group][aria-label="Terminal running"]').waitFor();
  ok(true, "advanced tab opens the raw terminal");
  await p.screenshot({ path: `${S}/25-advanced.png` });

  for (let i = 0; i < 2 && (await p.locator("[data-kind]").count()) === 0; i++)
    await p.getByRole("button", { name: /^HomeScreen/ }).click();
  // dismiss context chip
  await p.getByRole("tab", { name: "Chat" }).click();
  await p.getByRole("button", { name: "Don’t include this context" }).click();
  ok((await p.locator("[data-kind]").count()) === 0, "context chip can be dismissed");

  // NL
  await p.goto("http://localhost:1420/?mock=ready#/settings");
  await p.getByRole("radio", { name: "Nederlands" }).click();
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await p.getByText("Wat gaan we bouwen?").waitFor();
  ok(true, "chat is translated");
  ok(errs.length === 0, "no console errors " + errs.join(" | "));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
