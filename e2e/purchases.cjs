// Aankopen: product toevoegen, gating-plan, betaalmuur, "Add to app", begeleide winkelstappen en "Update app".
const { chromium } = require("playwright");
const S = process.argv[2];
const ok = (c, m) => {
  console.log((c ? "PASS " : "FAIL ") + m);
  if (!c) process.exitCode = 1;
};
(async () => {
  const b = await chromium.launch();
  const p = await (await b.newContext({ viewport: { width: 1440, height: 1000 }, locale: "en-US" })).newPage();
  const errs = [];
  p.on("pageerror", (e) => errs.push(e.message));
  p.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  await p.goto("http://localhost:1420/?mock=purchases#/project/friday_scores");
  await p.getByRole("navigation", { name: "Project menu" }).getByRole("button", { name: "Purchases" }).click();
  await p.getByText("No products yet").waitFor();
  ok(true, "empty state explains what to do");
  ok(await p.getByRole("button", { name: "Add to app" }).isDisabled(), "Add to app is disabled without products");

  // product 1: eenmalig
  await p.getByRole("button", { name: "Add a product" }).click();
  await p.locator("#p-name").fill("Pro");
  await p.locator("#p-price").fill("9.99");
  await p.waitForFunction(() => document.querySelector("#p-id")?.value === "pro");
  ok(
    await p.getByText("com.example.friday_scores.pro").first().isVisible(),
    "full product ID is shown and follows the name",
  );
  ok(await p.getByText(/Product IDs are permanent/).isVisible(), "permanent-ID warning before creating");
  await p.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  // product 2: jaarabonnement met proefperiode
  await p.getByRole("button", { name: "Add a product" }).click();
  await p.getByRole("radio", { name: /Subscription/ }).click();
  await p.locator("#p-name").fill("Pro Plus");
  await p.locator("#p-price").fill("29.99");
  await p.locator("#p-trial").fill("7");
  await p.waitForFunction(() => document.querySelector("#p-id")?.value === "pro_plus_yearly");
  ok(true, "subscription gets a consistent ID (pro_plus_yearly)");
  await p.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await p.getByText("com.example.friday_scores.pro_plus_yearly").first().waitFor();
  ok((await p.getByText("Draft").count()) >= 2, "each product shows its status per store");
  ok(
    (await p.locator("#provider").inputValue()) === "revenueCat",
    "RevenueCat is suggested when there is a subscription",
  );

  // gating
  await p.locator("#gate-desc").fill("Unlimited saved games and exporting");
  await p.locator("#gate-desc").blur();
  await p.getByRole("button", { name: "Let AI make the plan" }).click();
  await p
    .getByTestId("gating-plan")
    .getByText(/screen “Statistics”/)
    .waitFor({ timeout: 20000 });
  ok(true, "the plan is shown in plain language before anything is applied");
  ok(
    await p.getByRole("button", { name: "Add to app" }).isDisabled(),
    "Add to app stays disabled until the plan is approved",
  );
  ok(await p.getByText("Approve the plan for what goes behind the paywall.").isVisible(), "…and says why");
  await p.getByRole("button", { name: "Remove Export from the plan" }).click();
  await p.getByRole("button", { name: "Approve", exact: true }).click();
  await p.getByText("Approved", { exact: true }).waitFor();
  ok(
    (await p
      .getByTestId("gating-plan")
      .getByText(/button “Export”/)
      .count()) === 0,
    "an item can be removed before approving",
  );

  // betaalmuur
  await p.locator("#pw-headline").fill("Go Pro");
  await p.locator("#pw-headline").blur();
  await p.getByRole("img", { name: "Paywall preview" }).getByText("Go Pro").waitFor();
  const preview = await p.getByRole("img", { name: "Paywall preview" }).innerText();
  ok(/Price from the store/.test(preview), "the paywall shows 'price from the store', not a fixed amount");
  await p.screenshot({ path: `${S}/70-purchases.png`, fullPage: true });
  ok(
    await p.getByTestId("options-table").getByText("Statistics").first().isVisible(),
    "options table lists what each product unlocks",
  );
  await p.locator("#pw-tweak").fill("Make it calmer");
  await p.getByRole("button", { name: "Adjust", exact: true }).click();

  // toepassen
  ok(
    await p
      .getByTestId("apply-summary")
      .getByText(/create 2 product\(s\)/)
      .isVisible(),
    "plain-language summary of what will happen",
  );
  await p.getByRole("button", { name: "Add to app" }).click();
  const dlg = p.getByRole("alertdialog");
  ok(
    await dlg.getByText("com.example.friday_scores.pro_plus_yearly").isVisible(),
    "the confirmation lists the permanent product IDs",
  );
  await dlg.getByRole("button", { name: "Add to app" }).click();
  await p.getByText("Added to your app").waitFor({ timeout: 10000 });
  ok(true, "Add to app runs and reports success");
  await p
    .getByTestId("store-tasks")
    .getByText(/Google Play: Pro Plus/)
    .waitFor();
  ok((await p.getByText("In the app").count()) >= 2, "products are now 'In the app'");
  ok(
    await p.getByRole("button", { name: "Update app" }).isDisabled(),
    "the button turns into Update app and waits for changes",
  );

  // begeleide stap + update met risico
  await p
    .getByTestId("store-tasks")
    .locator("li", { hasText: "Google Play: Pro Plus" })
    .getByRole("button", { name: "I created it" })
    .click();
  await p.getByText("Created in the store").first().waitFor();
  ok(true, "a guided store step can be confirmed");
  await p.getByRole("button", { name: "Edit Pro", exact: true }).first().click();
  await p.locator("#p-desc").fill("Everything unlocked");
  await p.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  ok(
    await p
      .getByTestId("apply-summary")
      .getByText(/update 1 product\(s\)/)
      .isVisible(),
    "Update app only processes the difference",
  );
  await p.getByRole("button", { name: "Edit Pro Plus" }).click();
  ok(await p.locator("#p-id").isDisabled(), "the ID of a product that is already in the app can no longer be changed");
  await p.locator("#p-price").fill("39.99");
  await p.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await p.getByText(/Raising the price of “Pro Plus” affects people who already subscribe/).waitFor();
  ok(true, "a price increase for existing subscribers is flagged in plain words");
  await p.getByRole("button", { name: "Update app" }).click();
  ok(
    await p
      .getByRole("alertdialog")
      .getByText(/Raising the price/)
      .isVisible(),
    "the risk is repeated in the confirmation",
  );
  await p.screenshot({ path: `${S}/71-purchases-update.png`, fullPage: true });
  ok(errs.length === 0, "no console errors " + errs.join("|"));
  await b.close();
})().catch((e) => {
  console.error("FAIL", e.message.split("\n")[0]);
  process.exit(1);
});
