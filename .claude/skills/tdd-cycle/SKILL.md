# TDD Cycle

description: Enforces Test-Driven Development workflow. Auto-invoked when implementing new features, fixing bugs, or adding backend logic. Also available as /tdd-cycle.

---

Follow the TDD rules in CLAUDE.md. This skill provides the exact commands for each step.

## Red: Write Failing Test, Confirm Failure

```bash
# Rust
cd src-tauri && cargo test <test_name> -- --nocapture

# E2E
npx playwright test <test_file>
```

The test MUST fail. If it passes, it is not testing new behavior.

## Green: Write Minimum Implementation, Confirm Pass

Same commands as above. Fix the implementation, not the test.

## Refactor: Run Full Suite

```bash
# Rust
cd src-tauri && cargo test

# E2E
npx playwright test
```

All tests must pass. Zero regressions allowed. Then repeat from Red.
