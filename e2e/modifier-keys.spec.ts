import { test, expect } from "@playwright/test";

test.describe("Modifier Key Actions", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  // Helper: search, wait for results, verify no error after keypress
  async function searchAndAct(
    page: import("@playwright/test").Page,
    query: string,
    key: string
  ) {
    const input = page.locator(".search-input");
    await input.fill(query);
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    // Track console errors from the action
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.keyboard.press(key);
    await page.waitForTimeout(300);

    // No error notification should appear (element may not exist at all = success)
    await expect(
      page.locator(".notification", { hasText: "Action failed" })
    ).toHaveCount(0);

    return errors;
  }

  // --- Basic Enter (no modifier) ---

  test("Enter on app result executes without error", async ({ page }) => {
    const errors = await searchAndAct(page, "Firefox", "Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length,
      "execute_action should not produce console.error"
    ).toBe(0);
  });

  test("Shift+Enter on app result executes without error", async ({
    page,
  }) => {
    const errors = await searchAndAct(page, "Firefox", "Shift+Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  test("Ctrl+Enter on app result executes without error", async ({ page }) => {
    const errors = await searchAndAct(page, "Firefox", "Control+Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  test("Alt+Enter on app result executes without error", async ({ page }) => {
    const errors = await searchAndAct(page, "Firefox", "Alt+Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  // --- Math category ---

  test("Enter on math result executes without error", async ({ page }) => {
    const errors = await searchAndAct(page, "2+3", "Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length,
      "math Enter should not error"
    ).toBe(0);
    // Math+Enter should NOT trigger record_launch (no history recording for ephemeral categories)
    expect(
      errors.filter((e) => e.includes("Record launch failed")).length,
      "math should not record launch"
    ).toBe(0);
  });

  test("Shift+Enter on math result executes without error", async ({
    page,
  }) => {
    const errors = await searchAndAct(page, "2+3", "Shift+Enter");
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  // --- SSH category ---

  test("Enter on SSH result executes without error", async ({ page }) => {
    // ssh prefix shows SSH results if any exist in ~/.ssh/config
    const input = page.locator(".search-input");
    await input.fill("ssh");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    // SSH results depend on the system having SSH config — skip if none
    const count = await items.count();
    if (count === 0) {
      test.skip();
      return;
    }

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);

    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  // --- 1Password category ---

  test("Enter on onepass result executes without error", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("!github");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    // 1Password results depend on vault being loaded — skip if none
    const count = await items.count();
    if (count === 0) {
      test.skip();
      return;
    }

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);

    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  // --- File category ---

  test("Enter on file result executes without error", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" notes");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    if (count === 0) {
      test.skip();
      return;
    }

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);

    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  // --- Click uses none modifier ---

  test("clicking a result executes without error", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    const item = page.locator(".result-item:not(.empty)").first();
    await item.click();
    await page.waitForTimeout(300);

    await expect(
      page.locator(".notification", { hasText: "Action failed" })
    ).toHaveCount(0);

    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });
});
