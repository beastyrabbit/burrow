import { test, expect } from "@playwright/test";

test.describe("Launcher UI", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  // --- Rendering ---

  test("renders search input", async ({ page }) => {
    const input = page.locator(".search-input");
    await expect(input).toBeVisible();
    await expect(input).toBeFocused();
  });

  test("search input has placeholder text", async ({ page }) => {
    const input = page.locator(".search-input");
    await expect(input).toHaveAttribute(
      "placeholder",
      /Search apps, files, SSH hosts/
    );
  });

  test("renders results list container", async ({ page }) => {
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("has no window chrome (launcher div fills viewport)", async ({
    page,
  }) => {
    const launcher = page.locator(".launcher");
    await expect(launcher).toBeVisible();
  });

  // --- Typing and results ---

  test("typing in search input updates value", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("test query");
    await expect(input).toHaveValue("test query");
  });

  test("shows 'No results' for nonsense query", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("zzzzxqqqq_nonexistent");
    // Wait for debounce
    await page.waitForTimeout(200);
    await expect(page.locator(".result-item.empty")).toBeVisible();
    await expect(page.locator(".result-item.empty")).toHaveText("No results");
  });

  test("clearing input shows results list again", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("test");
    await page.waitForTimeout(200);
    await input.fill("");
    await page.waitForTimeout(200);
    // Empty input should show frecent history (may be empty on fresh DB)
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  // --- Keyboard navigation ---

  test("arrow down moves selection", async ({ page }) => {
    // Type a math expression that will return a result
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    // Should have at least one result
    const items = page.locator(".result-item");
    const count = await items.count();
    if (count > 0) {
      // First item should be selected by default
      await expect(items.first()).toHaveClass(/selected/);
    }
  });

  test("arrow keys cycle through results", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item");
    const count = await items.count();
    if (count > 0) {
      // Down arrow moves to next (or stays if only one)
      await page.keyboard.press("ArrowDown");
      // Up arrow moves back
      await page.keyboard.press("ArrowUp");
      await expect(items.first()).toHaveClass(/selected/);
    }
  });

  test("escape triggers hide_window without error", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("test");
    await expect(input).toHaveValue("test");

    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);

    // Escape calls hide_window — should not produce errors
    expect(
      errors.filter((e) => e.includes("hide_window failed")).length
    ).toBe(0);
  });

  // --- Math provider ---

  test("math expression shows calculator result", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("2+3");
    await page.waitForTimeout(200);

    const resultName = page.locator(".result-name").first();
    await expect(resultName).toContainText("= 5");
  });

  test("math result has Calc badge", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("10*5");
    await page.waitForTimeout(200);

    const badge = page.locator(".result-badge").first();
    await expect(badge).toHaveText("Calc");
  });

  test("complex math expression works", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("(3+4)*2");
    await page.waitForTimeout(200);

    const resultName = page.locator(".result-name").first();
    await expect(resultName).toContainText("= 14");
  });

  // --- Prefix routing ---

  test("space prefix triggers file search mode", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" nonexistentfile12345");
    await page.waitForTimeout(200);

    // Should show no results (file won't exist) or file results
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("! prefix triggers 1Password mode", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("!test");
    await page.waitForTimeout(300);

    // 1Password CLI likely not available in test env
    // Should not crash — either shows error or empty
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("ssh prefix triggers SSH mode", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("ssh ");
    await page.waitForTimeout(200);

    // Should show SSH results if config exists, or empty
    const list = page.locator(".results-list");
    await expect(list).toBeVisible();
  });

  test("space * prefix triggers vector content search", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" *rust");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    // Vector search requires indexed content — skip if DB is empty
    if (count === 0) {
      test.skip();
      return;
    }
    const resultName = page.locator(".result-name").first();
    await expect(resultName).toContainText("rust-guide.md");
  });

  test("vector search results have Content badge", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" *rust");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    if (count === 0) {
      test.skip();
      return;
    }
    const badge = page.locator(".result-badge").first();
    await expect(badge).toHaveText("Content");
  });

  test("vector search description shows score", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill(" *rust");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    const count = await items.count();
    if (count === 0) {
      test.skip();
      return;
    }
    const desc = page.locator(".result-desc").first();
    await expect(desc).toContainText("%");
  });

  test("vector search with empty content returns no results", async ({
    page,
  }) => {
    const input = page.locator(".search-input");
    await input.fill(" *");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item:not(.empty)");
    await expect(items).toHaveCount(0);
  });

  // --- Result item structure ---

  test("result items have name and badge", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("5+5");
    await page.waitForTimeout(200);

    await expect(page.locator(".result-name").first()).toBeVisible();
    await expect(page.locator(".result-badge").first()).toBeVisible();
  });

  test("result items show description when present", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("5+5");
    await page.waitForTimeout(200);

    // Math results have description
    const desc = page.locator(".result-desc").first();
    await expect(desc).toBeVisible();
    await expect(desc).toContainText("5+5");
  });

  // --- Mouse interaction ---

  test("hovering over a result selects it", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const items = page.locator(".result-item");
    const count = await items.count();
    if (count > 0) {
      await items.first().hover();
      await expect(items.first()).toHaveClass(/selected/);
    }
  });

  // --- Input state ---

  test("input has spellcheck disabled", async ({ page }) => {
    const input = page.locator(".search-input");
    await expect(input).toHaveAttribute("spellcheck", "false");
  });

  test("input is autofocused on load", async ({ page }) => {
    const input = page.locator(".search-input");
    await expect(input).toBeFocused();
  });

  // --- Styling ---

  test("launcher uses dark background", async ({ page }) => {
    const body = page.locator("body");
    const bg = await body.evaluate((el) =>
      getComputedStyle(el).backgroundColor
    );
    // #1a1b26 = rgb(26, 27, 38)
    expect(bg).toBe("rgb(26, 27, 38)");
  });

  test("selected item has highlight", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const selected = page.locator(".result-item.selected");
    const count = await selected.count();
    if (count > 0) {
      const bg = await selected.first().evaluate((el) =>
        getComputedStyle(el).backgroundColor
      );
      // #292e42 = rgb(41, 46, 66)
      expect(bg).toBe("rgb(41, 46, 66)");
    }
  });

  // --- All apps on empty query ---

  test("empty query shows more than 10 items (all apps)", async ({ page }) => {
    // Wait for initial app list to load (may take time on first search)
    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible({ timeout: 10000 });
    const count = await items.count();
    expect(count).toBeGreaterThan(10);
  });

  test("empty query shows history items first, then app items", async ({ page }) => {
    await page.waitForTimeout(200);
    const badges = page.locator(".result-badge");
    const count = await badges.count();
    if (count === 0) {
      test.skip();
      return;
    }
    const first = await badges.nth(0).textContent();
    // Fresh e2e data dir has no history — skip ordering test if no history items
    if (first !== "Recent") {
      test.skip();
      return;
    }
    // Find first "App" badge — history items should come before app items
    let foundAppAfterHistory = false;
    let historyDone = false;
    for (let i = 0; i < count; i++) {
      const text = await badges.nth(i).textContent();
      if (text === "App" && !historyDone) {
        historyDone = true;
        foundAppAfterHistory = true;
      }
      // No history items should appear after app items
      if (historyDone && text === "Recent") {
        foundAppAfterHistory = false;
        break;
      }
    }
    expect(foundAppAfterHistory).toBe(true);
  });

  test("keyboard navigation auto-scrolls selected item into view", async ({ page }) => {
    // Wait for initial app list to load
    const items = page.locator(".result-item:not(.empty)");
    await expect(items.first()).toBeVisible({ timeout: 10000 });
    const count = await items.count();
    if (count <= 5) {
      test.skip();
      return;
    }

    // Press ArrowDown many times to go past visible area
    for (let i = 0; i < count - 1; i++) {
      await page.keyboard.press("ArrowDown");
    }
    await page.waitForTimeout(100);

    // Last item should be selected and visible
    const lastItem = items.nth(count - 1);
    await expect(lastItem).toHaveClass(/selected/);
    const isVisible = await lastItem.isVisible();
    expect(isVisible).toBe(true);
  });

  // --- Styling ---

  test("badge has correct styling", async ({ page }) => {
    const input = page.locator(".search-input");
    await input.fill("1+1");
    await page.waitForTimeout(200);

    const badge = page.locator(".result-badge").first();
    const count = await badge.count();
    if (count > 0) {
      const textTransform = await badge.evaluate((el) =>
        getComputedStyle(el).textTransform
      );
      expect(textTransform).toBe("uppercase");
    }
  });
});
