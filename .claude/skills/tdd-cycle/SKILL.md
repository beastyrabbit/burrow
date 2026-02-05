# TDD Cycle

description: Enforces Test-Driven Development workflow. Auto-invoked when implementing new features, fixing bugs, or adding backend logic. Also available as /tdd-cycle.

---

You MUST follow this strict TDD workflow for every unit of functionality:

## Step 1: Write the Failing Test

- Identify the behavior to implement
- Write a test that asserts the expected outcome
- Place Rust tests in `#[cfg(test)]` module in the same file
- Place e2e tests in `e2e/` directory

## Step 2: Run the Test — Confirm Failure

- Rust: `cd src-tauri && cargo test <test_name> -- --nocapture`
- E2E: `npx playwright test <test_file>`
- The test MUST fail. If it passes, the test is wrong — it's not testing new behavior.

## Step 3: Write Minimum Implementation

- Write the smallest amount of code to make the test pass
- No extra features, no premature abstractions
- Follow existing patterns in the codebase

## Step 4: Run the Test — Confirm Pass

- Same command as Step 2
- The test MUST pass. If it fails, fix the implementation (not the test).

## Step 5: Run Full Suite

- Rust: `cd src-tauri && cargo test`
- E2E: `npx playwright test`
- All tests must pass. Zero regressions allowed.

## Step 6: Repeat

Go back to Step 1 for the next piece of functionality.

## Rules

- NEVER write implementation before the test exists
- NEVER skip running tests between steps
- NEVER modify a test to make it pass — fix the implementation instead
- Tests must use REAL data where possible (real filesystem, real DB, real apps)
- Include descriptive failure messages: `assert!(result.is_ok(), "expected ok, got: {result:?}")`
- Set minimum thresholds to catch silent regressions
