---
name: build-feature
description: "Usage: Build feature {spec-name} phase {phase-number}. Implements a single phase from the spec's task plan, iterating build-test cycles until the phase passes its constitution gate, then records memory, logs decisions, and commits."
version: 1.0
maturity: stable
input:
  properties:
    spec-name:
      type: string
      description: "Directory name of the feature spec under specs/ (e.g., 001-mcp-remote-agent-server)."
    phase-number:
      type: integer
      description: "Phase number to build from the spec's tasks.md (e.g., 6 for Phase 6)."
  required:
    - spec-name
    - phase-number
---

# Build Feature Skill

Implements a single phase from a feature specification's task plan. The workflow iterates through build-test cycles until the phase satisfies its constitution gate, then records session memory, logs architectural decisions, and commits all changes.

## Prerequisites

* A feature spec directory exists at `specs/${input:spec-name}/` containing `plan.md`, `spec.md`, and `tasks.md`
* The target phase exists in `tasks.md` with defined tasks
* The project compiles before starting (`cargo check` passes)
* The `.github/copilot-instructions.md` constitution and coding standards are accessible

## Remote Operator Integration (agent-intercom)

When the agent-intercom MCP server is reachable, all file modifications and status updates route through it so the remote operator can follow progress and approve changes via Slack.

### Availability Detection

At the start of every phase, call `ping` with a brief status message. If the call succeeds, agent-intercom is active — follow all remote workflow rules below. If it fails or times out, fall back to direct file writes and local-only operation.

### Status Broadcasting

Use `broadcast` (non-blocking) and `ping` (progress snapshots) throughout the phase to keep the operator informed. These calls never block the agent.

| When | Tool | Level | Message Pattern |
|---|---|---|---|
| Phase start | `broadcast` | `info` | `[BUILD] Starting phase {N}: {title} — {task_count} tasks` |
| Before each task | `broadcast` | `info` | `[TASK] {task_id}: {task_description}` |
| File created | `broadcast` | `info` | `[FILE] created: {file_path}` — include full file content in body |
| File modified | `broadcast` | `info` | `[FILE] modified: {file_path}` — include unified diff in body |
| After each task passes | `broadcast` | `success` | `[TASK] {task_id}: complete` |
| Task failure / retry | `broadcast` | `warning` | `[TASK] {task_id}: failed — {reason}, retrying` |
| After test suite run | `ping` | — | Include `progress_snapshot` with per-task `done`/`in_progress`/`pending` |
| Gate check result | `broadcast` | `success` or `error` | `[GATE] {gate_name}: PASS` or `FAIL — {details}` |
| Architectural decision | `broadcast` | `info` | `[ADR] {title} — {one-line rationale}` |
| Adversarial review complete | `broadcast` | `info` | `[REVIEW] Adversarial code review complete — {critical} critical, {high} high, {medium} medium, {low} low findings` |
| Adversarial fix applied | `broadcast` | `info` | `[FIX] {finding_id}: {file_path}` — include unified diff in body |
| Adversarial fixes complete | `broadcast` | `success` | `[REVIEW] Fixes applied — {applied} fixes, {deferred} deferred, compilation {status}` |
| Phase complete | `broadcast` | `success` | `[BUILD] Phase {N} complete — {tasks_done}/{tasks_total} tasks, commit {short_hash}` |

Post the first `broadcast` of each phase as a new top-level message and capture the returned `ts` value. Use that `ts` as the `thread_ts` parameter for all subsequent messages in the same phase to keep them grouped in a single Slack thread.

### File Change Approval Workflow

When agent-intercom is active, file creation and modification may proceed with direct writes. The three-step approval workflow is reserved for **destructive operations only** — file deletion, directory removal, or any operation that permanently removes content from the filesystem.

#### Destructive Operations (approval required)

Before deleting a file, removing a directory, or performing any other destructive filesystem operation, route the change through the approval workflow:

1. **`auto_check`** — Call with `tool_name` set to the destructive action (e.g., `"delete_file"`, `"remove_directory"`) and `context: { "file_path": "<relative_path>", "risk_level": "<low|high|critical>" }`. If `auto_approved: true`, execute the operation directly and skip steps 2–3.
2. **`check_clearance`** — Submit the proposal with a `title` describing the deletion, `diff` listing the files or directories to be removed, `file_path`, `description`, appropriate `risk_level`, and curated `snippets` (see Curating Code Snippets below). This call **blocks** until the operator responds.
3. **`check_diff`** — After receiving `status: "approved"`, call with the returned `request_id` to execute the deletion.

Response handling:
* `approved` → call `check_diff` to execute, then `broadcast` confirmation at `success` level.
* `rejected` → `broadcast` the rejection reason at `warning` level, adapt the approach based on the operator's `reason` field.
* `timeout` → treat as rejection. `broadcast` at `warning` level and do not retry automatically.

Risk level conventions:
* `low` — removing generated files, test fixtures, temporary artifacts
* `high` — removing configuration files, security modules (`diff/path_safety.rs`, `policy/`), Slack event handlers
* `critical` — removing database schema files (`persistence/schema.rs`), authentication/authorization code, CI/CD pipeline files

**One deletion per approval.** Submit each destructive operation as a separate `check_clearance` call.

#### Non-Destructive Operations (no approval needed)

File creation, modification, and all other non-destructive filesystem writes proceed directly without calling `check_clearance`. After each file write, call `broadcast` at `info` level with:
* `[FILE] {action}: {file_path}` as the message prefix, where `action` is `created` or `modified`.
* Include the unified diff for modifications or the full file content for new files in the broadcast message body so the operator can follow changes in real time.

These broadcasts are non-blocking and do not require operator response.

#### Curating Code Snippets for Operator Review

When calling `check_clearance`, always populate the **`snippets`** array with the most meaningful excerpts from the file being reviewed. The Slack UI renders snippets as inline, syntax-highlighted code blocks, giving the operator an immediately readable view without needing to open any attachment. Uploading the full file is a server-side fallback for when `snippets` is omitted — avoid it, because Slack desktop labels complex source files as "Binary".

**What to include in `snippets`:**

| Priority | Include when… | Example label |
|---|---|---|
| **Always** | The diff changes an existing function or method | `"handle() — main MCP tool entry point"` |
| **Always** | A public API signature changes (fn, struct, trait) | `"ApprovalRequest struct — field layout"` |
| **High** | Security-relevant code in scope of the change | `"path_safety::validate_path — traversal check"` |
| **High** | The surrounding context needed to understand the diff | `"fn build_approval_blocks — full context"` |
| **Optional** | Important callers of the changed code | `"SlackService::enqueue — caller site"` |
| **Skip** | Boilerplate, derives, trivial getters, auto-generated code | — |

**Snippet curation algorithm:**

1. Read the target file before writing your change.
2. Identify the chunk(s) you are modifying (functions, structs, trait impls).
3. For each chunk: extract the complete item — from its `pub`/`fn`/`struct`/`impl` line to its closing `}` — not just the changed lines. Context is what makes a diff reviewable.
4. Limit each snippet to the natural boundary of the item (don't truncate mid-function). The server truncates at 2,600 chars with a visible notice if a snippet is too long.
5. Supply 1–4 snippets per approval. More than 4 risks overwhelming the operator; fewer than 1 defeats the purpose.
6. Set `language` to the markdown code-fence label matching the file extension (e.g. `"rust"`, `"toml"`, `"typescript"`). Use `file_extension_language` conventions: `.rs` → `rust`, `.toml` → `toml`, `.json` → `json`, `.ts` → `typescript`, `.py` → `python`, etc.

**Function-boundary scoping rule:**

Each snippet must span *one complete function or method* — from its signature (including `pub`, `async`, generics) to its closing delimiter. If a change touches a single line inside a 30-line function, include all 30 lines. Never submit a snippet that starts or ends mid-function: the operator needs the full call site to reason about correctness, not a decontextualized fragment.

**Highlighting changed lines:**

Slack renders content inside backtick code fences (` ``` `) as literal preformatted text — no inline markdown is processed. `**bold**` and `_italic_` appear as literal asterisks and underscores. The only viable way to mark changed lines is to append an inline comment after each one:

| Change type | Annotation |
|---|---|
| Modified line | `// ← modified` |
| New line | `// ← new` |
| Deleted line | `// ← deleted` |

Use the comment prefix for the target language:

| Languages | Comment syntax |
|---|---|
| Rust, Go, Java, JS, TS, C, C++ | `//` |
| Python, Ruby, YAML, Shell | `#` |
| SQL, Lua | `--` |
| HTML, XML | `<!-- -->` |

Apply the annotation to every line that differs from the pre-change version of the function. Leave unchanged lines unannotated. A line already longer than ~90 characters may have the comment on the next line as a standalone remark (not standard practice — prefer one-line annotations).

**Example `check_clearance` call with snippets:**

```json
{
  "title": "Add retry_count field to ApprovalRequest",
  "diff": "--- a/src/models/approval.rs\n+++ b/src/models/approval.rs\n@@ -12,6 +12,7 @@ pub struct ApprovalRequest {\n     pub request_id: String,\n     pub file_path: String,\n     pub diff: String,\n+    pub retry_count: u32,\n     pub status: ApprovalStatus,\n }",
  "file_path": "src/models/approval.rs",
  "description": "Adds retry_count to track how many times a request has been resubmitted after a patch conflict.",
  "risk_level": "low",
  "snippets": [
    {
      "label": "ApprovalRequest struct — field layout after change",
      "language": "rust",
      "content": "pub struct ApprovalRequest {\n    pub request_id: String,\n    pub file_path: String,\n    pub diff: String,\n    pub retry_count: u32,  // ← new\n    pub status: ApprovalStatus,\n    pub created_at: DateTime<Utc>,\n    pub resolved_at: Option<DateTime<Utc>>,\n}"
    },
    {
      "label": "ApprovalRequest::new() — constructor updated to initialise retry_count",
      "language": "rust",
      "content": "impl ApprovalRequest {\n    pub fn new(request_id: String, file_path: String, diff: String) -> Self {\n        Self {\n            request_id,\n            file_path,\n            diff,\n            retry_count: 0,  // ← new\n            status: ApprovalStatus::Pending,\n            created_at: Utc::now(),\n            resolved_at: None,\n        }\n    }\n}"
    }
  ]
}
```

## Quick Start

Invoke the skill with both required parameters:

```text
Build feature 001-<spec-name> phase <phase-number>
```

The skill runs autonomously through all required steps, halting only on unrecoverable errors or constitution violations requiring human judgment.

## Parameters Reference

| Parameter      | Required | Type    | Description                                                        |
| -------------- | -------- | ------- | ------------------------------------------------------------------ |
| `spec-name`    | Yes      | string  | Directory name under `specs/` containing the feature specification |
| `phase-number` | Yes      | integer | Phase number from `tasks.md` to implement                          |

## Required Steps

### Step 1: Load Phase Context

* Read `specs/${input:spec-name}/tasks.md` and extract all tasks for the specified phase.
* Read `specs/${input:spec-name}/plan.md` for architecture, tech stack, and project structure.
* Read `specs/${input:spec-name}/spec.md` for user stories and acceptance scenarios relevant to this phase.
* Read `specs/${input:spec-name}/data-model.md` if it exists, for entity definitions and relationships.
* Read `specs/${input:spec-name}/contracts/` if it exists, for MCP tool JSON-RPC specifications and error contracts.
* Read `specs/${input:spec-name}/research.md` if it exists, for technical decisions and constraints.
* Read `specs/${input:spec-name}/quickstart.md` if it exists, for integration scenarios.
* Read `.github/copilot-instructions.md` for the project constitution, coding standards, and session memory requirements.
* Read `.github/agents/rust-engineer.agent.md` for language-specific engineering standards.
* Read `.github/agents/rust-mcp-expert.agent.md` for MCP protocol patterns, rmcp SDK usage, transport configuration, and tool/prompt/resource handler implementation.
* Read `.github/instructions/rust-mcp-server.instructions.md` for MCP server development best practices including error handling with `ErrorData`, state management, and testing patterns.
* Build a task execution list respecting dependencies: sequential tasks run in order, tasks marked `[P]` can run in parallel.
* Identify which tasks are tests and which are implementation; TDD order means test tasks execute before their corresponding implementation tasks.
* Report a summary of the phase scope: task count, estimated files affected, and user story coverage.
* **When agent-intercom is active**: call `broadcast` at `info` level with `[BUILD] Starting phase {N}: {title} — {task_count} tasks, {files_affected} estimated files`. Capture the returned `ts` and use it as `thread_ts` for all subsequent broadcasts in this phase.

### Step 2: Check Constitution Gate

* Read `specs/${input:spec-name}/plan.md` and locate the Constitution Check table.
* Verify every principle listed in the table is satisfied for the work about to begin.
* If `specs/${input:spec-name}/checklists/` exists, scan all checklist files and for each checklist count:
  * Total items: all lines matching `- [ ]` or `- [X]` or `- [x]`
  * Completed items: lines matching `- [X]` or `- [x]`
  * Incomplete items: lines matching `- [ ]`
* Create a status table:
```text
| Checklist   | Total | Completed | Incomplete | Status |
|-------------|-------|-----------|------------|--------|
| ux.md       | 12    | 12        | 0          | PASS   |
| test.md     | 8     | 5         | 3          | FAIL   |
| security.md | 6     | 6         | 0          | PASS   |
```
* If any constitution principle is violated or any required checklist is incomplete, halt and report the violation with actionable remediation steps.
* If all gates pass, proceed to Step 3.

### Step 3: Build Phase (Iterative)

Execute tasks in dependency order following TDD discipline:

1. For each task group (tests first, then implementation):
   * Classify the task type to determine which coding constraints apply:
     * Persistence tasks (touching `persistence/`, `models/`, schema): apply Database and Error Handling constraints from the Coding Standards section below.
     * MCP tasks (touching `mcp/server.rs`, `mcp/tools/`, `mcp/resources/`): apply MCP Tools and Error Handling constraints.
     * Slack tasks (touching `slack/`): apply Slack and Error Handling constraints.
     * Orchestrator tasks (touching `orchestrator/`): apply Async and Error Handling constraints.
     * Diff/Policy tasks (touching `diff/`, `policy/`): apply General Rust and Error Handling constraints.
     * IPC tasks (touching `ipc/`): apply General Rust and Error Handling constraints.
   * `broadcast` the task being started: `[TASK] {task_id}: {description}` at `info` level.
   * Read any existing source files that the task modifies.
   * For test tasks: write the test first, then run it and **confirm the test fails** before implementing the production code (red-green TDD).
   * Implement the task following the coding standards from the rust-engineer agent, injecting only the task-type-specific constraints identified above.
   * **When agent-intercom is active**: write files directly for creation and modification. After each file write, call `broadcast` at `info` level with `[FILE] {action}: {file_path}` and include the unified diff (for modifications) or full content (for new files) in the message body. For destructive operations (file deletion, directory removal), route through the approval workflow described in the Remote Operator Integration section.
   * After implementing each task, run `cargo check` to verify compilation.
   * If compilation fails, diagnose the error, fix it, and re-run `cargo check` until it passes.
   * `broadcast` task completion at `success` level, or failure with reason at `warning` level.
   * A task is complete only when `cargo check` passes **and** relevant tests pass. Mark the completed task as `[X]` in `specs/${input:spec-name}/tasks.md`.

2. Follow these implementation rules:
   * Setup tasks first (project structure, dependencies, configuration).
   * Test tasks before their corresponding implementation tasks (TDD).
   * Respect `[P]` markers: parallel tasks touching different files can be implemented together.
   * Sequential tasks (no `[P]` marker) must complete in listed order.
   * Tasks affecting the same files must run sequentially regardless of markers.

3. Error handling during build:
   * Halt execution if any sequential task fails. Do not proceed to the next task until the failure is resolved.
   * For parallel tasks `[P]`, continue with successful tasks and report failed ones.
   * Provide clear error messages with context for debugging.
   * If implementation cannot proceed, report the blocker and suggest next steps.
4. Track architectural decisions made during implementation for recording in Step 8.

### Step 4: Adversarial Code Review (Mandatory Gate)

This step is a mandatory quality gate. After all tasks in the phase have been built in Step 3, two independent adversarial code reviewers analyze the implementation for correctness, security, and constitution compliance before the code proceeds to testing.

1. **Collect phase artifacts**: Gather the list of all files created or modified during Step 3. For each file, capture the current content and the unified diff representing changes made during this phase.

2. **Dispatch adversarial reviewers**: Launch two adversarial code review subagents in parallel using `runSubagent`, each configured with a different model and a distinct review focus. Both receive the identical set of file contents, diffs, the project constitution (`.github/instructions/constitution.instructions.md`), and the phase's task list from `specs/${input:spec-name}/tasks.md`.

   Each reviewer produces a structured findings list. Limit each reviewer to 20 findings maximum to keep synthesis tractable.

   #### A. Reviewer — Code Correctness and Security (Gemini 3.1 Pro Preview)

   Invoke `runSubagent` with `model: "gemini-3.1-pro-preview"` and the following prompt:

   ```text
   You are an adversarial code reviewer focused on correctness, security,
   and edge-case handling in a Rust codebase.

   Analyze the provided source files and diffs from this build phase alongside
   the project constitution. Produce structured findings for:

   1. Logic errors, off-by-one mistakes, or incorrect control flow.
   2. Security vulnerabilities — path traversal, injection, unauthorized access,
      missing input validation.
   3. Error handling gaps — unwrap/expect usage, missing error propagation,
      swallowed errors.
   4. Edge cases not covered — empty inputs, boundary values, concurrent access,
      resource exhaustion.
   5. Constitution violations — unsafe code, missing doc comments, incorrect
      error handling patterns.
   6. Missing or inadequate test coverage for new code paths.

   For each finding, produce a table row with columns:
   ID (prefix CS), Severity (CRITICAL/HIGH/MEDIUM/LOW),
   File, Line(s), Summary, Recommended Fix.

   After the findings table, include:
   - A summary paragraph with your overall assessment of code quality.
   - A count of findings by severity level.

   Limit output to 20 findings. Prioritize by severity.
   ```

   When constructing the `runSubagent` prompt parameter, concatenate the reviewer prompt text above with the full content of each modified file, the corresponding diffs, and the constitution.

   #### B. Reviewer — Technical Quality and Architecture (GPT-5.3 Codex)

   Invoke `runSubagent` with `model: "gpt-5.3-codex"` and the following prompt:

   ```text
   You are an adversarial code reviewer focused on technical quality,
   architectural consistency, and performance in a Rust codebase.

   Analyze the provided source files and diffs from this build phase alongside
   the project constitution. Produce structured findings for:

   1. Architectural violations — code that breaks module boundaries, bypasses
      the repository layer, or introduces circular dependencies.
   2. Performance concerns — unnecessary allocations, blocking operations in
      async contexts, missing concurrency primitives.
   3. API design issues — inconsistent naming, leaky abstractions, missing
      pub(crate) visibility restrictions.
   4. Code duplication — repeated logic that should be extracted into shared
      functions or traits.
   5. Clippy and idiomatic Rust violations — non-idiomatic patterns that
      clippy pedantic would flag.
   6. Test quality — tests that don't assert meaningful behavior, missing
      negative test cases, brittle test assumptions.

   For each finding, produce a table row with columns:
   ID (prefix TQ), Severity (CRITICAL/HIGH/MEDIUM/LOW),
   File, Line(s), Summary, Recommended Fix.

   After the findings table, include:
   - A summary paragraph with your overall assessment of code quality.
   - A count of findings by severity level.

   Limit output to 20 findings. Prioritize by severity.
   ```

   When constructing the `runSubagent` prompt parameter, concatenate the reviewer prompt text above with the full content of each modified file, the corresponding diffs, and the constitution.

3. **Synthesize findings**: After both reviewers return, merge their findings into a unified report:
   * **Agreement elevation**: Findings identified by both reviewers are elevated in confidence. When reviewers assign different severities, adopt the higher severity.
   * **Deduplication**: Merge findings referencing the same file and line range with the same issue. Retain the strongest reasoning and most actionable recommendation.
   * **Severity normalization**:
     * *Critical*: Constitution violations, security vulnerabilities, data loss risks.
     * *High*: Logic errors, missing error handling, architectural violations agreed by both reviewers.
     * *Medium*: Performance concerns, code duplication, test quality issues, or findings identified by only one reviewer with strong justification.
     * *Low*: Style improvements, minor naming inconsistencies, or single-reviewer findings without corroboration.
   * **Consensus tagging**: Tag each finding as *agreed* (2/2 reviewers) or *single* (1/2).
   * Limit the unified findings list to 30 entries.

4. **Report findings**: Produce a summary of the adversarial review results including:
   * A reviewer summary table (model, focus area, findings count).
   * The unified findings table (ID, Severity, File, Lines, Summary, Recommended Fix, Consensus).
   * A metrics summary: total findings pre-deduplication, post-synthesis, agreement rate.

5. **When agent-intercom is active**: `broadcast` the review summary at `info` level: `[REVIEW] Adversarial code review complete — {critical} critical, {high} high, {medium} medium, {low} low findings`.

If the review produces zero critical or high findings, proceed directly to Step 6. Otherwise, proceed to Step 5 to apply fixes before testing.

### Step 5: Apply Adversarial Review Fixes (Mandatory Gate)

This step applies fixes from the adversarial code review in Step 4. It is mandatory when the review produced critical or high severity findings. Skip to Step 6 only when the review found zero critical or high findings.

1. **Severity-gated remediation**:
   * **Critical and High**: Apply the recommended fix directly to the affected source file. These fixes are mandatory and non-negotiable.
   * **Medium**: Apply the recommended fix. If the fix conflicts with the phase's design intent, document the reasoning for deferral in the review report.
   * **Low**: Record as suggestions in the review report. Do not apply unless the fix is trivial and risk-free.

2. **Apply fixes iteratively**: For each finding with an actionable recommendation:
   * Read the affected source file.
   * Apply the recommended code change.
   * **When agent-intercom is active**: write fixes directly and `broadcast` at `info` level with `[FIX] {finding_id}: {file_path}` and include the unified diff.
   * Run `cargo check` after each fix to verify compilation is maintained.
   * If a fix introduces a compilation error, diagnose and adjust the fix before proceeding.

3. **Verification pass**: After all fixes are applied:
   * Run `cargo check` to confirm the project still compiles.
   * Run `cargo test` to confirm no regressions were introduced.
   * If new test failures appear, diagnose whether the fix or the test is incorrect and resolve.
   * If the verification pass reveals new issues at critical or high severity, apply those fixes immediately (maximum two correction cycles to prevent infinite loops).

4. **Log remediation**: Record all applied fixes in the session's review log:
   * Finding ID, file path, description of the change, severity, and consensus tag.
   * Note any findings that were deferred with justification.

5. **When agent-intercom is active**: `broadcast` the remediation result at `success` level: `[REVIEW] Fixes applied — {applied} fixes, {deferred} deferred, compilation {status}`.

Proceed to Step 6 after all fixes are applied and the verification pass succeeds.

### Step 6: Test Phase (Mandatory Gate)

This step is a hard gate. The phase is not complete until all tests pass **and** both `cargo clippy` and `cargo fmt` exit cleanly. Do not skip lint or format checks under any circumstances, including context pressure or time constraints.
Run the full test suite and iterate until all checks pass:

1. Run `cargo test` to execute all test suites.
2. If any test fails:
   * Diagnose the failure from the test output.
   * Fix the implementation (not the test, unless the test itself has a bug).
   * **When agent-intercom is active**: write fixes directly and `broadcast` the diff at `info` level. If fixes involve deleting files, route through the approval workflow before re-running tests.
   * Re-run `cargo test` to verify the fix.
   * Repeat until all tests pass.
3. Run `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` to verify lint compliance.
4. If clippy reports warnings or errors, fix them and re-run until clean.
5. Run `cargo fmt --all -- --check` to verify formatting.
6. If formatting violations exist, run `cargo fmt --all` to auto-fix, then re-run `cargo fmt --all -- --check` to confirm the check passes.
7. If fixes in steps 3–6 introduced new test failures, return to step 1 and repeat the full cycle.
8. Call `ping` with a `progress_snapshot` showing all tasks and their status (`done`/`in_progress`/`pending`).
9. `broadcast` the gate result: `[GATE] Test phase: PASS` at `success` level, or `FAIL — {summary}` at `error` level.
10. Report final results: test suite counts, pass rates, clippy exit code, fmt exit code, and any notable findings.

All three checks (`cargo test`, `cargo clippy`, `cargo fmt --check`) must exit 0 before proceeding to Step 7. Return to Step 3 if test failures reveal missing implementation work. Continue iterating between Step 3 and Step 6 until build, test, lint, and format all pass cleanly.

### Step 7: Constitution Validation

Re-check the constitution after implementation is complete:

* Verify `#![forbid(unsafe_code)]` is maintained; no `unsafe` blocks introduced.
* Verify no `unwrap()` or `expect()` calls exist in library code paths.
* Verify all new public items have `///` doc comments.
* Verify error handling uses `AppError` variants from `src/errors.rs` with descriptive lowercase messages.
* Verify any new async code follows tokio patterns (no mutex guards held across `.await` points, `CancellationToken` for shutdown coordination).
* Verify all Slack message posting routes through the rate-limited message queue.
* Verify all file path operations validate against the workspace root via `starts_with()`.
* Verify test coverage aligns with the 80% target from the constitution.
* If any remediation changes were made during this step, write them directly (when agent-intercom is active, route only destructive operations through the approval workflow) and re-run the Step 6 gate checks (`cargo test`, `cargo clippy --all-targets -- -D warnings -D clippy::pedantic`, `cargo fmt --all -- --check`) to confirm the fixes did not introduce new lint or format violations. All three must exit 0 before proceeding.
* `broadcast` the constitution validation result: `[GATE] Constitution: PASS` at `success` level, or `FAIL — {violations}` at `error` level.

### Step 8: Record Architectural Decisions

For each significant decision made during the build phase:

* Create an ADR file in `docs/adrs/` following the naming convention `NNNN-{short-title}.md` where `NNNN` is the next sequential number (zero-padded to 4 digits).
* Each ADR includes:
  * Title describing the decision
  * Status (Accepted)
  * Context explaining the problem or situation
  * Decision made and rationale
  * Consequences (positive, negative, and risks)
  * Date and the phase/task that prompted the decision
* Decisions worth recording include: dependency choices, MCP tool design trade-offs, data model changes, SurrealDB workarounds, Slack interaction patterns, rmcp SDK patterns, error handling strategies, and performance trade-offs.
* **When agent-intercom is active**: call `broadcast` at `info` level for each ADR created: `[ADR] {NNNN}-{short-title} — {one-line rationale}`.
* Skip this step if no significant architectural decisions were made during the phase.

### Step 9: Record Session Memory (Mandatory Gate)

This step is a hard gate. The phase is not complete until the memory file exists on disk. Do not skip this step under any circumstances, including context pressure or time constraints.
Persist the full session details to `.copilot-tracking/memory/` following the project's session memory requirements:

* Create a memory file at `.copilot-tracking/memory/{YYYY-MM-DD}/{spec-name}-phase-{N}-memory.md` where the date is today and N is the phase number.
* The memory file includes:
  * Task Overview: phase scope and objectives
  * Current State: all tasks completed, files modified, test results
  * Important Discoveries: decisions made, failed approaches, SurrealDB or rmcp or slack-morphism quirks encountered
  * Next Steps: what the next phase should address, any open questions, known issues
  * Context to Preserve: source file references, agent references, unresolved questions
* Use the existing memory files in `.copilot-tracking/memory/` as format examples.
* After writing the file, verify it exists by reading it back. If the file is missing or empty, halt and retry.
* **When agent-intercom is active**: call `broadcast` at `info` level: `[MEMORY] Session recorded: {memory_file_path}`.

### Step 10: Pre-Commit Verification and Stage

1. Run `cargo fmt --all -- --check` to confirm formatting is clean. If it fails, run `cargo fmt --all` to auto-fix, then re-run the check.
2. Run `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` to confirm lint compliance. If it fails, fix violations and re-run until clean.
3. Run `cargo test` to confirm all tests still pass. If any test fails, fix and re-run.
4. If any fixes were applied in steps 1–3, repeat all three checks from step 1 to ensure no cascading violations. All three commands must exit 0 before proceeding.
5. Review all changes made during the phase to ensure they align with the completed tasks and constitution.
6. Review the ADRs created in Step 8 for clarity and completeness.
7. Review all steps to ensure that no steps have been missed and address any missing steps in the sequence before proceeding.
8. Review the session memory file for completeness and accuracy.

### Step 11: Stage, Commit, and Sync

Finalize all changes with a Git commit:

1. Accept all current diff changes (no interactive review).
2. Run `git add -A` to stage all modified, created, and deleted files.
3. Compose a commit message following these conventions:
   * Format: `feat({spec-name}): complete phase {N} - {phase title}`
   * `broadcast` the commit summary at `success` level: `[COMMIT] feat({spec-name}): complete phase {N} — {files_changed} files, {insertions}+/{deletions}-`
   * Body: list of completed task IDs and a brief summary of what was built
   * Footer: reference the spec path and any relevant ADR numbers
4. Run `git commit` with the composed message.
5. Run `git push` to sync the commit to the remote repository.
6. Report the commit hash and a summary of changes committed.

### Step 12: Compact Context (Mandatory Gate)
This step is a hard gate. The phase is not complete until context compaction has run and a checkpoint file exists. Do not skip this step, even if context space appears sufficient. When running in full-spec loop mode, the orchestrator verifies checkpoint existence before advancing to the next phase.
1. Run the `compact-context` skill (located at `.github/skills/compact-context/SKILL.md`).
2. Follow all steps defined in that skill: gather session state, write checkpoint, report, and compact.
3. Verify a checkpoint file was created in `.copilot-tracking/checkpoints/` during this execution. If missing, retry the compact-context skill.
### Phase Completion Signal
After Step 12 completes, the phase is fully done. Report the following completion signal for the orchestrator to consume:
* **Phase**: `{phase-number}` — `{phase title}`
* **Status**: COMPLETE
* **Memory file**: `.copilot-tracking/memory/{YYYY-MM-DD}/{spec-name}-phase-{N}-memory.md`
* **Checkpoint file**: `.copilot-tracking/checkpoints/{YYYY-MM-DD}-{HHmm}-checkpoint.md`
* **Commit hash**: `{hash}`
* **Tasks completed**: `{count}`
The orchestrator uses this signal to verify all gates passed before looping to the next phase.
## Troubleshooting

### Tests pass locally but fail in CI

Verify `rust-toolchain.toml` matches the CI configuration in `.github/workflows/ci.yml`. Check that the `[[test]]` entries in `Cargo.toml` include all external test files.

### Constitution violation detected

Return to Step 3 and fix the violation before proceeding. Common violations include `unwrap()` usage in library code, missing doc comments on public items, and `unsafe` blocks.

### Slack Socket Mode reconnection issues

If Slack interactions fail after reconnect, verify the `hello` event handler in `slack/client.rs` re-posts pending approval requests and continuation prompts per ADR-0011.

## Coding Standards

These rules are injected into each task based on its type classification in Step 3.

### General Rust

* `#![forbid(unsafe_code)]`
* Prefer borrowing over cloning
* Default to `pub(crate)`
* All public items require `///` doc comments

### Error Handling

* Use the `AppError` enum from `src/errors.rs` for all domain errors
* Variants: `Config`, `Db`, `Slack`, `Mcp`, `Diff`, `Policy`, `Ipc`, `PathViolation`, `PatchConflict`, `NotFound`, `Unauthorized`, `AlreadyConsumed`
* Map external errors via `From` impls or explicit `.map_err()`
* Error messages are lowercase and do not end with a period

### Database (SurrealDB)

* All DB access goes through `persistence/` repository modules (`approval_repo`, `session_repo`, `checkpoint_repo`, `prompt_repo`)
* No raw SurrealDB queries outside `persistence/`
* Use `kv-rocksdb` for production, `kv-mem` for tests
* Schema bootstrap uses `SCHEMAFULL` tables with idempotent DDL

### MCP Tools (rmcp)

* Tools follow the handler flow: validate session → parse params → execute domain logic → update session `last_tool`/`updated_at` → return JSON
* All 9 tools are always registered and visible; inapplicable calls return descriptive errors
* Blocking tools (`ask_approval`, `forward_prompt`, `wait_for_instruction`) use `tokio::sync::oneshot` channels
* Tool definitions use `#[tool]`/`#[tool_router]` macros from `rmcp` 0.5

### Slack (slack-morphism)

* Socket Mode with outbound-only WebSocket (no inbound firewall ports)
* All message posting routes through the rate-limited in-memory queue with exponential backoff
* Respect `Retry-After` headers from the Slack API
* Use Block Kit builders from `slack/blocks.rs` for all Slack messages
* Centralized authorization guard in `slack/events.rs` dispatcher — unauthorized users silently ignored
* Double-submission prevention via `chat.update` replacing buttons before handler dispatch

### Async (tokio)

* Target `tokio` 1 with the `full` feature set
* Use `tokio::task::spawn_blocking` for CPU-bound or blocking I/O (e.g., `keyring` credential lookups)
* Drop `MutexGuard`/`RwLockGuard` before `.await` points
* Use `tokio_util::sync::CancellationToken` for graceful shutdown coordination
* Use `tokio::process::Command` with `kill_on_drop(true)` for agent session processes

### Architecture Awareness

| Concern           | Approach                                                                                     |
| ----------------- | -------------------------------------------------------------------------------------------- |
| MCP SDK           | `rmcp` 0.5 — `ServerHandler` trait, `#[tool]`/`#[tool_router]` macros                       |
| Transport (stdio) | stdio via `rmcp` for direct agent connections                                                |
| Transport (HTTP)  | axum 0.8 with `StreamableHttpService` on `/mcp` for HTTP/SSE sessions                       |
| Slack             | `slack-morphism` Socket Mode                                                                 |
| Database          | SQLite via `sqlx` 0.8 — file-based production, in-memory (`":memory:"`) for tests           |
| Configuration     | TOML (`config.toml`) → `GlobalConfig`, credentials via keyring with env fallback             |
| Workspace policy  | JSON auto-approve rules (`.agentrc/settings.json`), hot-reloaded via `notify` file watcher  |
| Diff safety       | `diffy` 0.4 for unified diff parsing, `sha2` for integrity hashing, atomic writes via tempfile |
| Path security     | All paths canonicalized and validated via `starts_with(workspace_root)`                       |
| IPC               | `interprocess` crate — named pipes (Windows) / Unix domain sockets for `agent-intercom-ctl` |
| Shutdown          | `CancellationToken` — persist state, notify Slack, terminate children gracefully              |

---

Proceed with the user's request by executing the Required Steps in order for the specified `spec-name` and `phase-number`.
