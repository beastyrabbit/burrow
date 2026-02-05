import { test, expect } from "@playwright/test";

test.describe("Health Check", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("health indicator is visible when system is healthy", async ({ page }) => {
    await page.waitForTimeout(500);
    const indicator = page.locator(".health-indicator");
    await expect(indicator).toBeVisible();
    await expect(indicator).toHaveClass(/health-ok/);
    await expect(indicator).toHaveText("✓");
  });

  test("health indicator click shows notification with details", async ({ page }) => {
    await page.waitForTimeout(500);
    const indicator = page.locator(".health-indicator");
    await indicator.click();
    await page.waitForTimeout(300);

    const notification = page.locator(".notification");
    await expect(notification).toBeVisible();
    await expect(notification).toContainText("Ollama:");
    await expect(notification).toContainText("Vector DB:");
    await expect(notification).toContainText("Indexer:");
  });
});
