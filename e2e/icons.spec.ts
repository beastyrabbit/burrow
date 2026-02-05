import { test, expect } from "@playwright/test";

test.describe("Result Icons", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  // --- Icon element presence ---

  test("every result item has an icon element", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("fire");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    expect(count).toBeGreaterThan(0);

    for (let i = 0; i < count; i++) {
      const item = items.nth(i);
      const icon = item.locator(".result-icon, .result-icon-placeholder");
      await expect(icon).toBeVisible();
    }
  });

  test("frecent results have icon elements", async ({ page }) => {
    // Empty query shows frecent history
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    expect(count).toBeGreaterThan(0);

    for (let i = 0; i < count; i++) {
      const item = items.nth(i);
      const icon = item.locator(".result-icon, .result-icon-placeholder");
      await expect(icon).toBeVisible();
    }
  });

  // --- Category-specific icons (non-app categories use SVG fallbacks) ---

  test("math result shows calculator category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    // Should contain an SVG (the Lucide calculator icon)
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("SSH result shows terminal category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("ssh dev");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("chat result shows message category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?what is rust");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("1Password result shows key category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("!github");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("file result shows folder category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" notes");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("vector search result shows search category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" *rust");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  test("info hint shows info category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });

  // --- Icon sizing and layout ---

  test("icon placeholder is 32x32px", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const placeholder = page.locator(".result-icon-placeholder").first();
    const box = await placeholder.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.width).toBe(32);
    expect(box!.height).toBe(32);
  });

  test("icon does not cause layout shift between categories", async ({
    page,
  }) => {
    const input = page.locator(".search-input");

    // Get content position with math result
    await input.fill("1+1");
    await page.waitForTimeout(200);
    const mathContent = page.locator(".result-content").first();
    const mathBox = await mathContent.boundingBox();

    // Get content position with chat result
    await input.fill("?hello");
    await page.waitForTimeout(200);
    const chatContent = page.locator(".result-content").first();
    const chatBox = await chatContent.boundingBox();

    // Both should have the same left offset (icon takes same space)
    expect(mathBox).not.toBeNull();
    expect(chatBox).not.toBeNull();
    expect(mathBox!.x).toBe(chatBox!.x);
  });

  // --- App icons (mock mode has no backend icon resolution) ---

  test("app result without resolved icon shows app-window category icon", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("fire");
    await page.waitForTimeout(200);

    // Mock has no backend icon resolution, so apps fall back to category SVG
    const placeholder = page.locator(".result-icon-placeholder").first();
    await expect(placeholder).toBeVisible();
    const svg = placeholder.locator("svg");
    await expect(svg).toBeVisible();
  });
});
