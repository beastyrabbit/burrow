import { test, expect } from "@playwright/test";

const coworkResult = (page: import("@playwright/test").Page) =>
  page
    .locator(".result-item")
    .filter({ has: page.locator(".result-name", { hasText: /^cowork$/ }) });

test.describe("Special Commands", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("# alone shows all special commands", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#");
    await expect(coworkResult(page)).toBeVisible();
  });

  test("#cowork shows matching result with Special badge", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();
    await expect(
      page.locator(".result-item .result-badge", { hasText: "Special" })
    ).toBeVisible();
  });

  test("#cowork shows description", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(
      page.locator(".result-item .result-desc", {
        hasText: "Open kitty in ~/cowork and run Codex",
      })
    ).toBeVisible();
  });

  test("#nonexistent shows no results", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#nonexistent");
    const empty = page.locator(".result-item.empty");
    await expect(empty).toBeVisible();
    await expect(empty).toHaveText("No results");
  });

  test("partial match #cow filters correctly", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cow");
    await expect(coworkResult(page)).toBeVisible();
  });
});

test.describe("Secondary Input Mode", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("Enter on #cowork enters secondary mode", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Input should now have secondary class
    await expect(input).toHaveClass(/secondary/);

    // Secondary indicator should be visible
    await expect(page.locator(".secondary-indicator")).toBeVisible();
    await expect(page.locator(".secondary-name")).toHaveText("cowork");
  });

  test("secondary mode shows placeholder from input_spec", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode indicator
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    // Placeholder should be from input_spec
    await expect(input).toHaveAttribute(
      "placeholder",
      "Enter topic or press Enter to skip"
    );
  });

  test("secondary mode clears input", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    // Input should be empty
    await expect(input).toHaveValue("");
  });

  test("secondary mode hides results list", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    // Results list should not be visible
    await expect(page.locator(".results-list")).not.toBeVisible();
  });

  test("Escape in secondary mode exits and restores query", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Now in secondary mode
    await expect(input).toHaveClass(/secondary/);

    // Press Escape to exit
    await input.press("Escape");

    // Should no longer be in secondary mode
    await expect(input).not.toHaveClass(/secondary/);
    await expect(page.locator(".secondary-indicator")).not.toBeVisible();

    // Should show results list again
    await expect(page.locator(".results-list")).toBeVisible();

    // Query should be restored
    await expect(input).toHaveValue("#cowork");
  });

  test("empty Enter in secondary mode triggers execute_action with base command", async ({
    page,
  }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    // Press Enter with empty input
    await input.press("Enter");

    // Should exit secondary mode
    await expect(input).not.toHaveClass(/secondary/);

    // Should not produce any execute_action errors
    await page.waitForTimeout(300);
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  test("input + Enter in secondary mode triggers execute_action with input", async ({
    page,
  }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    // Type some input
    await input.fill("my-project");

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    // Press Enter with input
    await input.press("Enter");

    // Should exit secondary mode
    await expect(input).not.toHaveClass(/secondary/);

    // Should not produce any execute_action errors
    await page.waitForTimeout(300);
    expect(
      errors.filter((e) => e.includes("Execute action failed")).length
    ).toBe(0);
  });

  test("typing in secondary mode updates secondaryInput", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("#cowork");
    await expect(coworkResult(page)).toBeVisible();

    await input.press("Enter");

    // Wait for secondary mode
    await expect(page.locator(".secondary-indicator")).toBeVisible();

    // Type in secondary mode
    await input.fill("test-topic");
    await expect(input).toHaveValue("test-topic");
  });
});
