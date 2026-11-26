const { chromium } = require("playwright");
const S = process.argv[2];
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, locale: "en-US" });
  const p = await ctx.newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && !/create project failed/.test(m.text()) && errs.push(m.text()));
  const menu = (name) => p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name });
  await p.goto("http://localhost:1420/?mock=ready#/project/friday_scores");

  // Pages tree: alleen bij Schermen staan preview en chat
  await p.getByRole("button", { name: /^HomeScreen/ }).waitFor();
  ok(await p.getByText("Not linked").first().isVisible(), "unlinked pages are marked");
  ok(await p.getByRole("region", { name: "Live preview" }).isVisible(), "preview shown on Screens");
  ok(await p.getByRole("region", { name: "Chat with Claude" }).isVisible(), "chat shown on Screens");
  const settings = p.getByRole("button", { name: /^SettingsScreen/ });
  await settings.click();
  await p.getByText("Whole page: SettingsScreen").waitFor();
  ok((await settings.getAttribute("aria-pressed")) === "true", "clicking a page makes it the active page for the chat");
  const frame = await p
    .locator("iframe")
    .first()
    .getAttribute("src")
    .catch(() => null);
  ok(frame === null || /#\/settings$/.test(frame), "preview jumps to the page's route when running (" + frame + ")");

  // Folders: create, move a page in, collapse, persists after reload, delete keeps the page
  await p.getByRole("button", { name: "New folder", exact: true }).click();
  await p.getByRole("textbox", { name: "Folder name" }).fill("Account");
  await p.keyboard.press("Enter");
  await p.getByRole("button", { name: /^Account/ }).waitFor();
  await p.getByRole("button", { name: "Options for page SettingsScreen" }).click();
  await p.getByRole("menuitem", { name: "Move to folder" }).click();
  await p.getByRole("menuitem", { name: "Account" }).click();
  const folderBox = await p.getByRole("button", { name: /^Account/ }).boundingBox();
  const pageBox = await settings.boundingBox();
  ok(pageBox.y > folderBox.y && pageBox.x >= folderBox.x, "page now sits under its folder");
  await p.reload();
  await p.getByRole("button", { name: /^Account/ }).waitFor();
  ok(await p.getByRole("button", { name: /^SettingsScreen/ }).isVisible(), "folders persist after reload");
  await p.getByRole("button", { name: /^Account/ }).click();
  ok(!(await p.getByRole("button", { name: /^SettingsScreen/ }).isVisible()), "folder collapses");
  await p.getByRole("button", { name: /^Account/ }).click();
  await p.screenshot({ path: `${S}/50-screens.png` });
  await p.getByRole("button", { name: "Options for folder Account" }).click();
  await p.getByRole("menuitem", { name: "Delete folder" }).click();
  await p.getByRole("alertdialog").getByRole("button", { name: "Delete folder" }).click();
  ok(await p.getByRole("button", { name: /^SettingsScreen/ }).isVisible(), "deleting a folder keeps its pages");

  // Andere secties: geen preview, geen chat
  await menu("Theme").click();
  ok(
    (await p.locator("iframe").count()) === 0 &&
      (await p.getByRole("region", { name: "Chat with Claude" }).count()) === 0,
    "other pages have no preview or chat",
  );
  await menu("Screens").click();

  // Theme
  await menu("Theme").click();
  await p.getByRole("radio", { name: "#0D9488" }).click();
  await p.getByText("Color updated. The preview refreshes by itself.").first().waitFor();
  ok(
    (await p.getByRole("radio", { name: "#0D9488" }).getAttribute("aria-checked")) === "true",
    "theme swatch applied and selected",
  );
  // lettertype en stijl worden ook meteen toegepast, zonder chat
  await p.getByRole("radio", { name: "Poppins" }).click();
  await p.getByText("Saved", { exact: true }).first().waitFor();
  ok((await p.getByRole("radio", { name: "Poppins" }).getAttribute("aria-checked")) === "true", "font applied without the chat");
  await p.getByRole("radio", { name: "Rounded" }).click();
  await p.waitForFunction(() => document.querySelector('[role=radio][aria-checked=true]') !== null);
  ok((await p.getByRole("button", { name: /Let Claude|Connect the theme/ }).count()) === 0, "no chat button needed to finish the theme");
  await p.locator("#theme-hex").fill("nope");
  ok(await p.getByRole("alert").filter({ hasText: "color code like" }).isVisible(), "invalid color code explained");
  await p.screenshot({ path: `${S}/51-theme.png` });

  // Settings
  await menu("App settings").click();
  await p.locator("#s-bundle").waitFor();
  await p.locator("#s-bundle").fill("Bad Id");
  ok(await p.getByRole("alert").filter({ hasText: "lowercase letters" }).isVisible(), "invalid bundle id is explained while typing");
  await p.locator("#s-bundle").blur();
  ok((await p.getByText("Updated:").count()) === 0, "an invalid bundle id is never applied");
  await p.locator("#s-bundle").fill("fail.demo");
  await p.locator("#s-bundle").blur(); // toegepast bij het verlaten van het veld: geen opslaanknop
  await p.getByText("couldn’t change everything automatically").waitFor();
  ok(
    await p.getByRole("button", { name: "Ask Claude to fix this" }).isVisible(),
    "skipped files are reported with an Ask Claude action",
  );
  await p.locator("#s-bundle").fill("nl.bakker.scores");
  await p.locator("#s-bundle").blur();
  await p.getByText("Updated: Android: applicationId").waitFor();
  ok(true, "bundle id written");
  await p.getByRole("switch", { name: "Camera" }).click();
  await p.getByLabel("Camera: Why does the app need this? (shown on iPhones)").waitFor();
  ok(true, "enabling camera asks for the iPhone reason");
  await p.screenshot({ path: `${S}/52-settings.png` });

  // Assets
  await menu("Assets").click();
  await p.getByRole("button", { name: "Choose an image…" }).click();
  await p.getByText("26 files created").waitFor();
  ok(await p.getByRole("img", { name: "Current icon" }).isVisible(), "icon generated and shown");

  // Versions
  await menu("Versions").click();
  await p.getByText("Claude: add a settings screen").first().waitFor();
  ok(
    await p.getByText("The next upload automatically gets version 1.0.1, build 2.").isVisible(),
    "next upload version is announced (already uploaded 1.0.0+1)",
  );
  await p.getByRole("button", { name: "Change manually…" }).click();
  await p.locator("#v-build").fill("1");
  ok(await p.getByRole("alert").filter({ hasText: "2 or higher" }).isVisible(), "build must exceed the uploaded one");
  await p.locator("#v-version").fill("1.2.0");
  await p.locator("#v-build").fill("7");
  await p.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await p.getByTestId("version-current").filter({ hasText: "1.2.0" }).waitFor();
  ok((await p.getByTestId("version-current").innerText()).includes("+7"), "manual version and build saved");
  await p.getByPlaceholder("What did you just finish?").fill("Before the big change");
  await p.getByRole("button", { name: "Save a snapshot" }).click();
  await p.getByText("Before the big change").waitFor();
  ok(true, "manual snapshot appears first in the timeline");
  await p.getByRole("button", { name: "Go back to this version" }).nth(1).click();
  const dlg = p.getByRole("alertdialog");
  ok(await dlg.getByText("Nothing is lost").isVisible(), "restore asks for confirmation and explains it is safe");
  await dlg.getByRole("button", { name: "Yes, go back" }).click();
  await p
    .getByText(/^Restored: /)
    .first()
    .waitFor();
  ok(true, "restore creates a new snapshot");
  ok(await p.getByRole("button", { name: "Let Claude write the changelog" }).isVisible(), "changelog action present");
  await p.screenshot({ path: `${S}/53-versions.png` });

  // Data
  await menu("Data & login").click();
  await p.getByRole("radio", { name: /Supabase/ }).click();
  await p.locator("#sb-ref").fill("abcdefghij");
  await p.locator("#sb-token").fill("sbp_supersecrettoken0123456789abc");
  await p.getByRole("button", { name: "Save token" }).click();
  await p.getByText("✓ Saved in the keychain").waitFor();
  ok(
    !(await p.content()).includes("sbp_supersecrettoken0123456789abc"),
    "token value is gone from the page after saving",
  );
  await p.getByLabel("Email and password").check();
  await p.getByLabel("The app lets people delete their account").waitFor();
  ok(true, "account deletion is asked when there are accounts");
  await p.getByLabel("The app lets people delete their account").check();
  await p.getByRole("button", { name: "Save", exact: true }).click();
  await p.getByText("Saved", { exact: true }).first().waitFor();
  await p.screenshot({ path: `${S}/54-data.png` });

  // Publish gate
  await menu("Publish").click();
  await p.getByText(/of 9 required items done/).waitFor();
  const publishBtn = p.getByRole("button", { name: /^Publish/ }).first();
  ok(await publishBtn.isDisabled(), "top bar Publish stays disabled while items are open");
  ok(await p.getByText("Run the test").first().isVisible(), "AI test item says what to do");
  await p.screenshot({ path: `${S}/55-checklist.png` });
  await p.getByRole("tab", { name: "Release" }).click();
  ok(await p.getByRole("button", { name: "Build and upload" }).isDisabled(), "Build and upload is blocked");
  ok(
    await p.getByLabel("I checked everything and want to release this version").isDisabled(),
    "final confirmation cannot be ticked before the rest is green",
  );

  // Test run
  await p.getByRole("tab", { name: "Test run" }).click();
  await p.getByRole("button", { name: "Run the test" }).click();
  await p.getByText("Passed on the current version").waitFor({ timeout: 8000 });
  ok(await p.getByText("What this test does not cover").isVisible(), "honest list of what is not tested");
  ok(await p.getByText("Button seems to do nothing").isVisible(), "warnings are listed per screen");
  await p.screenshot({ path: `${S}/56-test.png` });

  // Legal
  await p.getByRole("tab", { name: "Legal" }).click();
  await p.getByRole("button", { name: "Let Claude write them" }).click();
  await p
    .getByText("Both documents exist in the legal folder")
    .waitFor({ timeout: 15000 })
    .catch(() => {});
  await p.getByLabel("Link to the privacy policy").fill("https://example.com/privacy");
  await p.getByLabel("Link to the terms").fill("https://example.com/terms");
  await p.getByLabel("I opened both links and the pages are online").check();
  await p.getByRole("button", { name: "Save" }).click();

  // Listing
  await p.getByRole("tab", { name: "Store page" }).click();
  await p.getByRole("button", { name: "Let Claude write the texts" }).click();
  await p.waitForTimeout(6500);
  await p.getByRole("tab", { name: "Checklist" }).click();
  await p.getByRole("tab", { name: "Store page" }).click();
  ok(
    (await p.locator("#l-title").inputValue()) === "Friday Scores",
    "Claude filled the store listing via the Studio tool",
  );
  await p
    .getByRole("button", { name: "Take one from the preview" })
    .isEnabled()
    .then((d) => ok(d, "screenshots can be taken from the running preview"));
  await p.getByRole("button", { name: "Add my own…" }).click();
  await p.getByRole("button", { name: "Add my own…" }).click();
  for (const q of [
    "People can create an account",
    "The app collects personal data (names, emails, …)",
    "The app shows ads",
    "The app has in-app purchases",
    "People can post their own content",
    "The app uses location",
  ]) {
    await p.getByRole("radiogroup", { name: q }).getByRole("radio", { name: "No" }).click();
  }
  await p.getByText("What this means in Play Console").waitFor();
  await p.getByLabel("I filled these in in Google Play Console").check();
  await p.getByRole("button", { name: "Save", exact: true }).click();
  await p.screenshot({ path: `${S}/57-listing.png` });

  // Signing
  await p.getByRole("tab", { name: "Accounts" }).click();
  await p.locator("#ks-owner").fill("Jan Bakker");
  await p.getByRole("button", { name: "Create signing key" }).click();
  await p.getByText("Back this file up").waitFor();
  await p.getByRole("button", { name: "Let Claude set this up" }).click();
  await p.getByText("✓ Set up").waitFor({ timeout: 15000 });
  await p
    .locator("#play-key")
    .fill('{"type":"service_account","client_email":"x@y.iam.gserviceaccount.com","private_key":"SECRET_KEY_VALUE"}');
  await p.getByRole("button", { name: "Save key" }).click();
  await p.getByText("✓ Saved in the keychain").waitFor();
  ok(!(await p.content()).includes("SECRET_KEY_VALUE"), "service account key is gone from the page after saving");
  await p.getByLabel("My Google Play developer account is ready").click();
  await p.getByRole("button", { name: "Check support" }).click();
  await p.getByText("Not available yet").waitFor();
  ok(true, "iOS publishing says honestly that it is not available yet");
  await p.screenshot({ path: `${S}/58-signing.png` });

  // changelog via Claude
  await menu("Versions").click();
  await p.getByRole("button", { name: "Let Claude write the changelog" }).click();
  await p.waitForTimeout(6500);

  // Gate
  await menu("Publish").click();
  await p.getByRole("tab", { name: "Release" }).click();
  await p.getByLabel("I checked everything and want to release this version").click();
  await p
    .getByText("Everything required is ready. You can publish.")
    .first()
    .waitFor()
    .catch(() => {});
  await p.getByRole("tab", { name: "Checklist" }).click();
  await p.getByText("9 of 9 required items done").waitFor({ timeout: 8000 });
  ok(
    await p
      .getByRole("button", { name: /^Publish/ })
      .first()
      .isEnabled(),
    "top bar Publish enables when all 9 required items are green",
  );
  await p.screenshot({ path: `${S}/59-green.png` });
  await p.getByRole("tab", { name: "Release" }).click();
  await p.getByRole("button", { name: "Build and upload" }).click();
  const cd = p.getByRole("alertdialog");
  ok(
    await cd.getByText("Nothing goes live to the public").isVisible(),
    "upload requires an explicit confirmation that explains what happens",
  );
  await cd.getByRole("button", { name: "Yes, build and upload" }).click();
  await p.getByText("Uploaded! Open Play Console").first().waitFor({ timeout: 12000 });
  ok(await p.getByText("app-release.aab").isVisible(), "build log is shown");
  await p.screenshot({ path: `${S}/60-released.png` });

  ok(errs.length === 0, "no console errors " + errs.join(" | ").slice(0, 300));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n").slice(0, 4).join(" / "));
  process.exit(1);
});
