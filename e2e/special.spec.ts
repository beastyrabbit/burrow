import { test, expect } from "@playwright/test";

test.describe("Special Commands", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("# alone shows all special commands", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#");
    await expect(
      page.locator(".result-item", { hasText: "cowork" })
    ).toBeVisible();
  });

  test("#cowork shows matching result with Special badge", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(
      page.locator(".result-item", { hasText: "cowork" })
    ).toBeVisible();
    await expect(
      page.locator(".result-item .result-badge", { hasText: "Special" })
    ).toBeVisible();
  });

  test("#cowork shows description", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(
      page.locator(".result-item .result-desc", {
        hasText: "Open kitty in ~/cowork and run cc",
      })
    ).toBeVisible();
  });

  test("#nonexistent shows no results", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#nonexistent");
    await page.waitForTimeout(200);
    const empty = page.locator(".result-item.empty");
    await expect(empty).toBeVisible();
    await expect(empty).toHaveText("No results");
  });

  test("partial match #cow filters correctly", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cow");
    await expect(
      page.locator(".result-item", { hasText: "cowork" })
    ).toBeVisible();
  });

  test("Enter on special command triggers execute_action", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(
      page.locator(".result-item", { hasText: "cowork" })
    ).toBeVisible();

    const logs: string[] = [];
    page.on("console", (msg) => logs.push(msg.text()));

    await input.press("Enter");
    await page.waitForTimeout(200);
    expect(logs.some((l) => l.includes("execute_action"))).toBe(true);
  });
});
