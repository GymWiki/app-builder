// Doorloop: "Apple aan" -> "Geheimen toevoegen" -> volledig ingevulde sectie -> checklistpunt wordt groen.
const { chromium } = require("playwright");
const S = process.argv[2] || "/tmp/e2e-out";
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
const P8 = Buffer.from(
  "-----BEGIN PRIVATE KEY-----\nMIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgfake0123456789\n-----END PRIVATE KEY-----\n",
);
const SA = Buffer.from(
  JSON.stringify({
    type: "service_account",
    project_id: "demo-proj",
    client_email: "studio-bot@demo-proj.iam.gserviceaccount.com",
    private_key: "-----BEGIN PRIVATE KEY-----\nABCDEF0123456789\n-----END PRIVATE KEY-----\n",
  }),
);
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1440, height: 1000 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  const menu = (name) => p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name });
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");
  await menu("Integrations").click();

  // 10b: kaarten, status, aan/uit
  await p.getByRole("heading", { name: "Apple (App Store)" }).waitFor();
  const apple = p.locator("#integration-apple");
  ok(await apple.getByText("Off", { exact: true }).isVisible(), "a switched-off card says Off");
  ok((await apple.getByRole("button", { name: /secrets/i }).count()) === 0, "no secrets button while it is off");
  await p.screenshot({ path: `${S}/70-integrations-off.png` });
  await p.getByRole("switch", { name: "Apple (App Store) on or off" }).click();
  await apple.getByText("On, secrets missing").waitFor();
  ok(true, "Apple on: status 'On, secrets missing'");
  const add = apple.getByRole("button", { name: "Add secrets" });
  ok(await add.isVisible(), "an 'Add secrets' button appears");
  await p.screenshot({ path: `${S}/71-apple-on.png` });

  // Add secrets -> Secrets page, scrolled to and marking the Apple heading
  await add.click();
  const sec = p.locator("#secrets-apple");
  await sec.getByRole("heading", { name: "Apple (App Store)" }).waitFor();
  ok(await sec.getByText("Missing", { exact: true }).isVisible(), "the section starts as Missing (✗)");
  ok(/ring-2/.test((await sec.getAttribute("class")) || ""), "the heading's section is highlighted briefly");
  ok((await p.locator("#secrets-supabase").count()) === 0, "only activated integrations get a heading");
  ok(
    await p
      .getByText("Instructions coming soon")
      .count()
      .then((n) => n === 0),
    "no guide button while no guide exists",
  );
  ok(
    await p.getByText(/Public values .* may be in your app\. Secret values/).isVisible(),
    "the public/secret difference is explained",
  );
  ok(
    (await sec.getByText("secret", { exact: true }).count()) === 1 &&
      (await sec.getByText("public", { exact: true }).count()) === 3,
    "fields are labelled public or secret",
  );

  // validation with friendly messages, one field at a time
  const issuer = sec.getByLabel(/Issuer ID/);
  await issuer.fill("abc");
  ok(await sec.getByText(/does not look right/).isVisible(), "instant validation: friendly message with a next step");
  await issuer.fill("69a6de7e-1234-47e3-e053-5b8c7c11a4d1");
  await issuer.press("Enter");
  await p.getByText("Issuer ID saved.").waitFor();
  await sec.getByLabel(/Key ID/).fill("ABCDE12345");
  await sec.getByRole("button", { name: "Save" }).first().click();
  await p.getByText("Key ID saved.").waitFor();
  await sec.getByLabel(/Team ID/).fill("TEAM123456");
  await sec.getByRole("button", { name: "Save" }).first().click();
  ok(await sec.getByText("Incomplete", { exact: true }).isVisible(), "partly filled = Incomplete (⚠)");

  // file field: wrong type first, then the real .p8
  const file = sec.locator('input[type="file"]');
  await file.setInputFiles({ name: "key.txt", mimeType: "text/plain", buffer: P8 });
  await sec.getByText(/not the right type of file/).waitFor();
  ok(true, "the wrong file type is refused with a next step");
  await file.setInputFiles({ name: "AuthKey_ABCDE12345.p8", mimeType: "application/octet-stream", buffer: P8 });
  await sec.getByText("File saved: AuthKey_ABCDE12345.p8").waitFor();
  ok(await sec.getByText("Complete", { exact: true }).isVisible(), "all fields in: Complete (✓)");
  ok(!(await p.content()).includes("fake0123456789"), "the key file content is never on the page");
  await sec.getByRole("button", { name: "Test connection" }).click();
  await sec.getByText(/details look right/).waitFor();
  ok(true, "test connection gives a plain-language result");
  ok(await sec.getByText("Used by").isVisible(), "'used by' is shown");
  await p.screenshot({ path: `${S}/72-apple-complete.png`, fullPage: false });

  // toegankelijkheid: alles bedienbaar heeft een naam
  const unnamed = await p.evaluate(() =>
    [
      ...document.querySelectorAll(
        "main button, main input, main select, main [role=switch], section button, section input, section select",
      ),
    ]
      .filter((el) => !el.closest("[aria-hidden=true]"))
      .filter((el) => {
        const h = el;
        const name =
          h.getAttribute("aria-label") ||
          h.getAttribute("aria-labelledby") ||
          (h.labels && h.labels.length) ||
          h.textContent?.trim() ||
          h.getAttribute("title") ||
          h.getAttribute("placeholder");
        return !name;
      })
      .map((el) => el.outerHTML.slice(0, 80)),
  );
  ok(unnamed.length === 0, "every button and field on the Secrets page has an accessible name " + unnamed.join("|"));

  // secret text field: hidden by default, show needs a confirmation, hides itself
  await menu("Integrations").click();
  await p.getByRole("switch", { name: "GitHub on or off" }).click();
  await p.locator("#integration-github").getByRole("button", { name: "Add secrets" }).click();
  const gh = p.locator("#secrets-github");
  await gh.getByLabel(/Access token/).fill("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
  await gh.getByRole("button", { name: "Save" }).first().click();
  await p.getByText("Access token saved.").waitFor();
  ok(!(await p.content()).includes("ghp_abcdefghijkl"), "a saved secret is not in the page");
  ok(await gh.getByText("••••••••••••").isVisible(), "secret values are masked");
  await gh.getByRole("button", { name: "Show" }).click();
  await p.getByRole("alertdialog").getByText("Show this value?").waitFor();
  await p.getByRole("alertdialog").getByRole("button", { name: "Show" }).click();
  await gh.getByRole("button", { name: /Hide \(\d+s\)/ }).waitFor();
  ok((await gh.textContent()).includes("ghp_abcdefghijkl"), "after confirming, the value is shown for a moment");
  await gh.getByRole("button", { name: /Hide/ }).click();
  ok(!(await gh.textContent()).includes("ghp_abcdefghijkl"), "and can be hidden again");
  // delete asks for confirmation
  await gh.getByRole("button", { name: "Delete" }).click();
  await p
    .getByRole("alertdialog")
    .getByText(/cannot be brought back/)
    .waitFor();
  await p.getByRole("alertdialog").getByRole("button", { name: "Cancel" }).click();
  ok(await gh.getByText("••••••••••••").isVisible(), "cancelling a delete keeps the value");

  // toetsenbord: een schakelaar bedien je met Tab en spatie
  await menu("Integrations").click();
  const sw = p.getByRole("switch", { name: "Firebase on or off" });
  await sw.focus();
  await p.keyboard.press("Space");
  await p
    .locator("#integration-firebase")
    .getByText(/On, secrets missing/)
    .waitFor();
  ok(true, "integrations can be switched with the keyboard");
  await sw.focus();
  await p.keyboard.press("Space");
  await p.getByRole("alertdialog").getByRole("button", { name: "Cancel" }).focus();
  await p.keyboard.press("Enter");
  await p.locator("#integration-firebase").getByText("On, secrets missing").waitFor();
  await sw.focus();
  await p.keyboard.press("Space");
  await p.getByRole("alertdialog").getByRole("button", { name: "Switch off and keep the secrets" }).click();
  await p.locator("#integration-firebase").getByText("Off", { exact: true }).waitFor();
  ok(true, "the switch-off dialog works with the keyboard and Cancel keeps it on");

  // dependency: Google Play needs Android signing
  await menu("Integrations").click();
  await p.getByRole("switch", { name: "Google Play on or off" }).click();
  await p.getByRole("alertdialog").getByText("Google Play needs more").waitFor();
  await p.getByRole("alertdialog").getByRole("button", { name: "Switch on both" }).click();
  await p
    .locator("#integration-android_signing")
    .getByText(/On, secrets missing/)
    .waitFor();
  ok(true, "switching on Google Play also offers (and does) Android signing");

  // checklist before: secrets missing with a jump button
  await menu("Publish").click();
  await p.getByRole("tab", { name: "Checklist" }).click();
  const item = p.locator("li", { hasText: "Accounts and signing" }).first();
  ok(
    await item.getByText(/Secrets missing: Android signing › Upload keystore/).isVisible(),
    "checklist: 'Secrets missing' names the integration and field",
  );
  ok(
    await item.getByText(/Secrets missing: Google Play › Service account key/).isVisible(),
    "checklist lists every missing secret once",
  );
  await item.getByRole("button", { name: "Add secrets" }).click();
  await p.locator("#secrets-android_signing").getByRole("heading", { name: "Android signing" }).waitFor();
  ok(true, "the checklist button jumps to the right heading");

  // fill Android signing (generated) and Google Play (service account)
  const as = p.locator("#secrets-android_signing");
  await as.getByRole("button", { name: "Create it for me" }).click();
  await as.getByLabel("Your name").fill("Jan Bakker");
  await as.getByRole("button", { name: "Create signing key" }).click();
  await p.getByText("Signing key created and saved.").waitFor();
  ok(
    await as.getByText("Complete", { exact: true }).isVisible(),
    "Android signing is complete after Studio created the key",
  );
  const gp = p.locator("#secrets-google_play");
  await gp
    .locator('input[type="file"]')
    .setInputFiles({ name: "key.json", mimeType: "application/json", buffer: Buffer.from("{}") });
  await gp.getByText(/missing a part|not the right kind/).waitFor();
  ok(true, "a wrong JSON file is refused");
  await gp.locator('input[type="file"]').setInputFiles({ name: "key.json", mimeType: "application/json", buffer: SA });
  await gp
    .getByText(/studio-bot@demo-proj/)
    .first()
    .waitFor();
  ok(
    await gp.getByText("Complete", { exact: true }).isVisible(),
    "Google Play is complete; the account e-mail is shown, the key is not",
  );
  ok(!(await p.content()).includes("ABCDEF0123456789"), "the key itself is never shown");
  await p.screenshot({ path: `${S}/73-secrets-all.png` });

  // checklist after: secrets are gone from the item; finish the rest and the item turns green
  await menu("Publish").click();
  await p.getByRole("tab", { name: "Accounts" }).click();
  await p.getByRole("button", { name: "Let Claude set this up" }).click();
  await p.getByText("✓ Set up").waitFor({ timeout: 15000 });
  await p.getByLabel("My Google Play developer account is ready").click();
  await p.getByRole("tab", { name: "Checklist" }).click();
  const item2 = p.locator("li", { hasText: "Accounts and signing" }).first();
  await item2
    .getByText("✓")
    .first()
    .waitFor({ state: "attached" })
    .catch(() => {});
  ok((await item2.getByText(/Secrets missing/).count()) === 0, "no secrets are asked for twice");
  ok(
    /opacity-80/.test((await item2.getAttribute("class")) || ""),
    "the checklist item 'Accounts and signing' is now green (done)",
  );
  await p.screenshot({ path: `${S}/74-checklist-green.png` });

  // leak: blocked and explained
  await p.evaluate(() => {
    globalThis.__studioMockLeaks = [
      { file: "AuthKey_X.p8", line: 0, kind: "secret_file", tracked: false, movable: true },
    ];
  });
  await menu("Secrets").click();
  await p.getByRole("heading", { name: "A secret was found in your files" }).waitFor();
  await p.getByRole("button", { name: "Move to the safe" }).click();
  await p.getByRole("alertdialog").getByRole("button", { name: "Move to the safe" }).click();
  await p.getByText("Moved to the safe and removed from your files.").waitFor();
  ok(true, "a leaked key file can be moved to the safe");
  await p.screenshot({ path: `${S}/75-leak.png` });

  // switching off with the keep/delete choice
  await menu("Integrations").click();
  await p.getByRole("switch", { name: "GitHub on or off" }).click();
  const dlg = p.getByRole("alertdialog");
  await dlg.getByText("Switch off GitHub?").waitFor();
  ok(
    await dlg.getByRole("button", { name: "Switch off and keep the secrets" }).isVisible(),
    "switching off offers: keep the secrets",
  );
  ok(
    await dlg.getByRole("button", { name: /delete this app's secrets/ }).isDisabled(),
    "global-only integrations cannot delete shared secrets from here",
  );
  await dlg.getByRole("button", { name: "Switch off and keep the secrets" }).click();
  await p.locator("#integration-github").getByText("Off", { exact: true }).waitFor();
  await menu("Secrets").click();
  ok((await p.locator("#secrets-github").count()) === 0, "a switched-off integration loses its heading");

  // Dutch
  await p.goto("http://localhost:1420/?mock=ready&lng=nl#/settings");
  ok(errs.length === 0, "no console errors " + errs.join("|").slice(0, 200));
  await b.close();
})();
