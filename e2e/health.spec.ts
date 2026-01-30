import { test, expect } from "@playwright/test";

test.describe("Health Check", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("no health indicator when system is healthy", async ({ page }) => {
    // Mock returns all-healthy, so ! should not appear
    await page.waitForTimeout(500);
    const indicator = page.locator(".health-indicator");
    await expect(indicator).toHaveCount(0);
  });

  test(":health shows in settings list", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(":health");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items).toHaveCount(1);
    const name = page.locator(".result-name").first();
    await expect(name).toContainText(":health");
  });

  test(": prefix shows 6 settings including health", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(":");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items).toHaveCount(6);
  });

  test(":health action shows health status in notification", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(":health");
    await page.waitForTimeout(200);

    await page.keyboard.press("Enter");
    await page.waitForTimeout(300);

    const notification = page.locator(".notification");
    await expect(notification).toContainText("Ollama: OK");
  });
});
