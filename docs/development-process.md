# Development Process

These rules are mandatory. Follow them in order. Do not skip steps.

---

## Phase 1: Plan

**Do this before writing any code, any tests, or any files other than the plan itself.**

1. Read the user's request carefully
2. Read [projects/state.md](projects/state.md) to understand the current codebase
3. Ask clarifying questions if the request is ambiguous — do not assume
4. Create a branch: `git checkout -b <short-slug>`
5. Write a plan file at `docs/projects/plans/<short-slug>.md` containing:
   - **Branch:** the branch name created in step 4
   - **What** is being built and why
   - **Which files** will be created or modified (be specific)
   - **How** it will be implemented: data model, API shape, UI flow
   - **Open questions or tradeoffs** you are not sure about
6. Show the plan to the user and **stop**
7. **Wait for explicit approval** — a response like "looks good", "yes", or "go ahead"
7. Do not write any implementation code or tests until you receive that approval

If the user requests changes to the plan, update the plan file and show it again. Repeat until approved.

---

## Phase 2: Implementation (TDD)

Work through the plan one behavior at a time. For each behavior:

### Step 1 — Write a failing test

Write the test before writing any implementation code.

The test must fail **because the behavior does not exist yet** — not just because the code does not compile. If the code does not compile, that is not yet a failing test; it is an incomplete test. Get it to compile first (with stub implementations returning dummy values), then confirm it fails at runtime.

A good failing test:
- Calls the real function or module being built (not a placeholder)
- Asserts the specific outcome the behavior should produce
- Fails with a clear message that points to the missing behavior
- Would pass once the behavior is correctly implemented and fail if it regresses

A bad failing test:
- Fails only because it calls a function that does not exist yet
- Asserts `true` or uses `assert!(result.is_ok())` without checking what is inside
- Would pass with any non-panicking implementation
- Tests the wrong thing (e.g., tests the test setup rather than the code)

Show the failing test output to the user before moving on.

### Step 2 — Write the minimum code to make it pass

Implement only what is needed to make the test pass. Do not add features, abstractions, or handling for cases the test does not cover yet.

Run the test and confirm it passes.

### Step 3 — Refactor if needed

Clean up the implementation while keeping the test green. Run the test again after refactoring to confirm it still passes.

### Step 4 — Repeat

Move to the next behavior in the plan. Write the next failing test. Do not skip ahead.

---

## Rules

- **Never write implementation code before a test exists for it**
- **Never move to the next behavior before the current test passes**
- **Never show the user a passing test without having first shown the failing version**
- Tests live inline with the code they cover: `#[cfg(test)]` blocks in the same `.rs` file
- Use `#[tokio::test]` for async tests
- Use descriptive test names that state the scenario and expected outcome: `test_expired_signature_is_rejected`, not `test_signature`
- Use `expect("message")` instead of `unwrap()` so failures are readable
- Test return types can be `-> Result<(), E>` to allow using `?` inside tests

See [testing.md](testing.md) for Rust-specific patterns.

---

## Adding a New Feature (Typical Flow)

1. Add struct to `src/models/mod.rs` (see [models.md](models.md))
2. Add migration: `migrations/YYYYMMDDHHMMSS_description.sql` (see [migrations.md](migrations.md))
3. Add async CRUD functions to `src/db.rs` returning `Result<T, sqlx::Error>`
4. Add `#[server]` functions to `src/frontend/server_fns.rs`
5. Create page: `src/frontend/pages/mypage.rs`
6. Register in `src/frontend/pages/mod.rs`: `mod mypage; pub use mypage::MyPage;`
7. Register route in `src/frontend/mod.rs`: add to `use pages::` import and `Route` enum

---

## Evolving this process

This document should change as we learn what works. Either party can propose a change at any time — proposals are especially natural after a project wraps up, but don't wait.

**Proactively propose changes** when you notice:
- A step that caused unnecessary friction or delay
- A pattern that worked especially well and is not captured here
- A rule that did not fit the situation

To propose a change: describe it in plain text and explain why. No plan doc needed. Once confirmed, update this file.

Process changes follow the same confirm-before-change rule — propose first, update after the user agrees.

---

## Keeping project docs current

After each project completes:
- Add a file to [projects/completed/](projects/completed/) named `YYYYMMDD-short-slug.md`
- Update [projects/state.md](projects/state.md) if features or architecture changed
- Remove the idea file from [projects/ideas/](projects/ideas/) if it originated there

When the user mentions a new idea, add a file to [projects/ideas/](projects/ideas/) before it is forgotten.

## Writing idea files

Idea files describe **what the user will be able to do** or **what problem gets solved** — not how it will be built. Keep them abstract and user-focused.

A good idea file answers:
- What can the user do that they cannot do today?
- What problem or friction does this remove?

A good idea file does **not** include:
- Implementation approach, data models, or API design
- File names, module structure, or technology choices
- Anything that belongs in a plan

Implementation details belong in the plan, which is written once the idea is approved and work begins. If an idea file starts to look like a plan, trim it back.

**Example of what to avoid:** "Add a `notifications` table with columns `id`, `device_id`, `message`, `created_at` and expose it via a new server function `get_notifications()`..."

**Example of the right level:** "Users can receive notifications when a device has not checked in for a configurable period of time."
