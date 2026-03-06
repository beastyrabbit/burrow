import { test, expect } from "@playwright/test";

// Helper to build mock get_output responses for page.route()
function mockGetOutput(page: import("@playwright/test").Page, responses: any[]) {
  let pollIndex = 0;
  return page.route("**/api/get_output", async (route) => {
    const resp =
      pollIndex < responses.length
        ? responses[pollIndex++]
        : { lines: [], done: true, exit_code: 0, total: responses.reduce((s, r) => s + r.lines.length, 0) };
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(resp),
    });
  });
}

const codexUrl = (label = "mock-codex") =>
  `/?view=output&label=${label}&title=Test&format=codex_json`;

test.describe("Codex Output View", () => {
  test("renders markdown table from agent message", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "item.completed",
              item: {
                id: "msg_1",
                type: "agent_message",
                text: "## Summary\n| PR | Status |\n|---|---|\n| #1 | Merged |",
              },
            }),
          },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.completed",
              usage: { input_tokens: 1000, output_tokens: 200 },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 3,
      },
    ]);

    await page.goto(codexUrl());

    // Markdown table should render
    await expect(page.locator(".codex-message-content table")).toBeVisible();
    await expect(page.locator(".codex-message-content th", { hasText: "PR" })).toBeVisible();
    await expect(page.locator(".codex-message-content th", { hasText: "Status" })).toBeVisible();
    await expect(page.locator(".codex-message-content td", { hasText: "Merged" })).toBeVisible();
  });

  test("renders command card with collapsible output", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "item.started",
              item: {
                id: "cmd_1",
                type: "command_execution",
                command: "kubectl get pods",
                aggregated_output: "",
                exit_code: null,
                status: "in_progress",
              },
            }),
          },
        ],
        done: false,
        exit_code: null,
        total: 2,
      },
      {
        lines: [
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "item.completed",
              item: {
                id: "cmd_1",
                type: "command_execution",
                command: "kubectl get pods",
                aggregated_output: "NAME  READY\npod1  1/1",
                exit_code: 0,
                status: "completed",
              },
            }),
          },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.completed",
              usage: { input_tokens: 500, output_tokens: 100 },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 4,
      },
    ]);

    await page.goto(codexUrl());

    // Command card should show command text
    const card = page.locator(".codex-card");
    await expect(card).toBeVisible();
    await expect(card.locator(".codex-card-command")).toHaveText("kubectl get pods");

    // Exit badge should show exit 0
    await expect(card.locator(".codex-card-exit")).toHaveText("exit 0");

    // Click header to expand output
    await card.locator(".codex-card-header").click();
    await expect(card.locator(".codex-card-output")).toBeVisible();
    await expect(card.locator(".codex-card-output")).toContainText("pod1  1/1");
  });

  test("shows Completed status (not green Done)", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.completed",
              usage: { input_tokens: 100, output_tokens: 50 },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 2,
      },
    ]);

    await page.goto(codexUrl());

    const status = page.locator(".output-status");
    await expect(status).toHaveText("Completed");
    await expect(status).toHaveClass(/status-completed/);
    // Should NOT be green "Done"
    await expect(status).not.toHaveText("Done");
  });

  test("shows Failed status on turn.failed", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.failed",
              error: { message: "Something went wrong" },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 2,
      },
    ]);

    await page.goto(codexUrl());

    await expect(page.locator(".output-status")).toHaveText("Failed");
    await expect(page.locator(".output-status")).toHaveClass(/status-failed/);
  });

  test("events tab shows raw JSONL lines with stream metadata", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          { stream: "stderr", text: "some debug info" },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.completed",
              usage: { input_tokens: 100, output_tokens: 50 },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 3,
      },
    ]);

    await page.goto(codexUrl());

    // Switch to Events tab
    await page.locator(".codex-tab", { hasText: "Events" }).click();

    // Should show raw lines
    const events = page.locator(".codex-events");
    await expect(events).toBeVisible();
    await expect(events).toContainText("turn.started");
    await expect(events).toContainText("some debug info");

    // Stderr lines should have distinct styling
    const stderrSpan = events.locator(".event-stderr");
    await expect(stderrSpan).toBeVisible();
    await expect(stderrSpan).toContainText("some debug info");
  });

  test("token usage footer displays when turn completes", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.completed",
              usage: { input_tokens: 1234, output_tokens: 567, cached_input_tokens: 890 },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 2,
      },
    ]);

    await page.goto(codexUrl());

    const usage = page.locator(".codex-usage");
    await expect(usage).toBeVisible();
    await expect(usage).toContainText("1,234 in");
    await expect(usage).toContainText("567 out");
    await expect(usage).toContainText("890 cached");
  });

  test("tabs switch between Output and Events", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          { stream: "stdout", text: '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}' },
        ],
        done: true,
        exit_code: 0,
        total: 2,
      },
    ]);

    await page.goto(codexUrl());

    // Output tab is active by default
    const outputTab = page.locator(".codex-tab", { hasText: "Output" });
    const eventsTab = page.locator(".codex-tab", { hasText: "Events" });
    await expect(outputTab).toHaveClass(/active/);
    await expect(eventsTab).not.toHaveClass(/active/);

    // Switch to Events
    await eventsTab.click();
    await expect(eventsTab).toHaveClass(/active/);
    await expect(outputTab).not.toHaveClass(/active/);
    await expect(page.locator(".codex-events")).toBeVisible();

    // Switch back to Output
    await outputTab.click();
    await expect(outputTab).toHaveClass(/active/);
  });

  test("last agent message is expanded, prior ones collapsed", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "item.completed",
              item: { id: "msg_1", type: "agent_message", text: "First message" },
            }),
          },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "item.completed",
              item: { id: "msg_2", type: "agent_message", text: "Second message (latest)" },
            }),
          },
          { stream: "stdout", text: '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}' },
        ],
        done: true,
        exit_code: 0,
        total: 4,
      },
    ]);

    await page.goto(codexUrl());

    const messages = page.locator(".codex-agent-message");
    await expect(messages).toHaveCount(2);

    // First message should be collapsed
    await expect(messages.first()).toHaveClass(/collapsed/);

    // Last message should be expanded (not collapsed)
    await expect(messages.last()).not.toHaveClass(/collapsed/);
    await expect(messages.last().locator(".codex-message-content")).toContainText(
      "Second message (latest)"
    );
  });

  test("error banner shows on turn.failed", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: '{"type":"turn.started"}' },
          {
            stream: "stdout",
            text: JSON.stringify({
              type: "turn.failed",
              error: { message: "Rate limit exceeded" },
            }),
          },
        ],
        done: true,
        exit_code: 0,
        total: 2,
      },
    ]);

    await page.goto(codexUrl());

    await expect(page.locator(".codex-error-banner")).toBeVisible();
    await expect(page.locator(".codex-error-banner")).toContainText("Rate limit exceeded");
  });

  test("plain output view still works (no format param)", async ({ page }) => {
    await mockGetOutput(page, [
      {
        lines: [
          { stream: "stdout", text: "hello from plain output" },
        ],
        done: true,
        exit_code: 0,
        total: 1,
      },
    ]);

    // No format param → should render plain OutputView
    await page.goto("/?view=output&label=mock-plain&title=Plain");

    // Should use the plain output view, not codex
    await expect(page.locator(".output-view")).toBeVisible();
    await expect(page.locator(".codex-output")).not.toBeVisible();
    await expect(page.locator(".output-content")).toContainText("hello from plain output");
  });
});
