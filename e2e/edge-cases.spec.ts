import { test, expect } from "@playwright/test";

test.describe("Edge Cases", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("rapid typing triggers debounce correctly", async ({ page }) => {
    const input = page.locator(".search-input");

    // Type characters rapidly
    await input.pressSequentially("1+1", { delay: 10 });
    // Wait for debounce to settle
    await page.waitForTimeout(300);

    // Should show math result
    const resultName = page.locator(".result-name").first();
    await expect(resultName).toContainText("= 2");
  });

  test("very long input string does not crash", async ({ page }) => {
    const input = page.locator(".search-input");
    const longString = "a".repeat(500);
    await input.fill(longString);
    await page.waitForTimeout(300);

    // Should show results list (even if empty)
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("special characters in query do not crash", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("test<>&\"'");
    await page.waitForTimeout(200);

    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("switching between prefixes works", async ({ page }) => {
    const input = page.locator(".search-input");

    // Start with settings prefix
    await input.fill(":");
    await page.waitForTimeout(200);
    const settingsItems = page.locator(".result-item:not(.empty)");
    await expect(settingsItems).toHaveCount(6);

    // Switch to math
    await input.fill("2+2");
    await page.waitForTimeout(200);
    const mathResult = page.locator(".result-name").first();
    await expect(mathResult).toContainText("= 4");

    // Switch to file search
    await input.fill(" nonexistent");
    await page.waitForTimeout(200);
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("empty after typing returns to initial state", async ({ page }) => {
    const input = page.locator(".search-input");

    await input.fill("1+1");
    await page.waitForTimeout(200);

    await input.fill("");
    await page.waitForTimeout(200);

    // Should return to history view
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });
});
