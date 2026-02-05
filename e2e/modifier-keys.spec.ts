import { test, expect } from "@playwright/test";

test.describe("Modifier Key Actions", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  // --- Basic Enter (no modifier) ---

  test("Enter on app result invokes execute_action with none modifier", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    // Capture console log from mock
    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=none"))).toBe(true);
  });

  test("Shift+Enter invokes execute_action with shift modifier", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Shift+Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=shift"))).toBe(true);
  });

  test("Ctrl+Enter invokes execute_action with ctrl modifier", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Control+Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=ctrl"))).toBe(true);
  });

  // --- Math category (no-op for plain Enter, copy for Shift/Ctrl) ---

  test("Enter on math result invokes execute_action but skips record_launch", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("2+3");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Enter");
    await page.waitForTimeout(100);

    // Math should still invoke execute_action (backend handles no-op)
    expect(logs.some((l) => l.includes("execute_action") && l.includes("category=math"))).toBe(true);
    // But should NOT invoke record_launch
    expect(logs.some((l) => l.includes("record_launch"))).toBe(false);
  });

  test("Shift+Enter on math result invokes execute_action with shift", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("2+3");
    await page.waitForTimeout(200);

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Shift+Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=shift"))).toBe(true);
  });

  // --- SSH category ---

  test("Enter on SSH result invokes execute_action", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("ssh devbox");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("category=ssh"))).toBe(true);
  });

  // --- 1Password category ---

  test("Enter on onepass result invokes execute_action", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("!github");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("category=onepass"))).toBe(true);
  });

  // --- File category ---

  test("Enter on file result invokes execute_action", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" notes");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("category=file"))).toBe(true);
  });

  // --- Alt modifier ---

  test("Alt+Enter invokes execute_action with alt modifier", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await page.keyboard.press("Alt+Enter");
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=alt"))).toBe(true);
  });

  // --- Click uses none modifier ---

  test("clicking a result invokes execute_action with none modifier", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("Firefox");
    await page.waitForTimeout(200);

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    const item = page.locator(".result-item:not(.empty)").first();
    await item.click();
    await page.waitForTimeout(100);

    expect(logs.some((l) => l.includes("execute_action") && l.includes("modifier=none"))).toBe(true);
  });
});
