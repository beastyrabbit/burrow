import { test, expect } from "@playwright/test";

test.describe("Chat Mode", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("? prefix shows chat hint when empty", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?");
    await expect(
      page.locator(".result-item", { hasText: "Type a question after ?" })
    ).toBeVisible();
  });

  test("? prefix with text shows Ask prompt", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?what is rust");
    await expect(
      page.locator(".result-item", { hasText: "Ask: what is rust" })
    ).toBeVisible();
    await expect(
      page.locator(".result-item .result-badge", { hasText: "Chat" })
    ).toBeVisible();
  });

  test("pressing Enter on chat result shows answer", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?what is rust");
    await expect(
      page.locator(".result-item", { hasText: "Ask:" })
    ).toBeVisible();
    await input.press("Enter");
    // Chat answer should appear (may take time with real AI backend)
    await expect(page.locator(".chat-answer")).toBeVisible({ timeout: 30000 });
    // Verify it has non-empty content (real AI response, not a specific mock string)
    const text = await page.locator(".chat-answer").textContent();
    expect(text?.trim().length).toBeGreaterThan(0);
  });

  test("pressing Enter on empty ? hint does not trigger chat", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?");
    await expect(
      page.locator(".result-item", { hasText: "Type a question after ?" })
    ).toBeVisible();
    await input.press("Enter");
    await expect(page.locator(".chat-answer")).not.toBeVisible();
  });

  test("chat answer clears when query changes", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("?hello");
    await expect(
      page.locator(".result-item", { hasText: "Ask:" })
    ).toBeVisible();
    await input.press("Enter");
    await expect(page.locator(".chat-answer")).toBeVisible({ timeout: 30000 });

    await input.fill("firefox");
    await expect(page.locator(".chat-answer")).not.toBeVisible();
  });
});
