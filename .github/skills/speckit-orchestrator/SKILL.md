---
name: speckit-orchestrator
description: "Usage: Run spec workflow {feature-description}. Orchestrates the spec-kit feature specification lifecycle from specify through analysis, compacting context between stages, and hands off to build-orchestrator for implementation."
version: 1.0
maturity: stable
input:
  properties:
    feature-description:
      type: string
      description: "Natural language description of the feature to specify. Passed as $ARGUMENTS to the specify stage."
    mode:
      type: string
      description: "Execution mode: full (all stages), from (resume from a stage), single (one stage only). Defaults to full."
      enum: [full, from, single]
      default: full
    start-stage:
      type: string
      description: "Stage to start from when mode is 'from' or 'single'. Ignored in full mode."
      enum: [specify, clarify, plan, behavior, tasks, analyze, operator-review]
    skip-clarify:
      type: boolean
      description: "Skip the interactive clarification stage. Defaults to false."
      default: false
  required:
    - feature-description
---

# Spec-Kit Orchestrator Skill

Orchestrates the spec-kit feature specification lifecycle as a single autonomous workflow. Drives each stage of the Spec-Driven Development (SDD) pipeline in order, validates exit criteria before advancing, compacts the context window between major stages using the compact-context skill, and hands off to the build-orchestrator when the specification is complete and ready for implementation.

This skill assumes the project constitution (`.specify/memory/constitution.md`) already exists. Use `/speckit.constitution` separately if one needs to be created or updated before starting feature work.

## Prerequisites

* The workspace root contains `.specify/` directory with templates, scripts, and constitution
* `.specify/memory/constitution.md` exists (run `/speckit.constitution` first if missing)
* `.github/agents/speckit.*.agent.md` files exist for all pipeline stages
* `.github/skills/compact-context/SKILL.md` exists for context management
* Git repository is initialized with a clean working tree (or acceptable uncommitted changes)

## Quick Start

Invoke the skill with a feature description:

```text
Run spec workflow {feature-description}
```

Or resume from a specific stage:

```text
Run spec workflow {feature-description} from behavior
```

The skill runs autonomously through all stages, halting only when user input is required (clarification questions) or on unrecoverable errors.

## Parameters Reference

| Parameter             | Required | Type    | Description                                                                    |
| --------------------- | -------- | ------- | ------------------------------------------------------------------------------ |
| `feature-description` | Yes      | string  | Natural language feature description passed to the specify stage               |
| `mode`                | No       | string  | `full` (default), `from` (resume at stage), or `single` (one stage only)       |
| `start-stage`         | No       | string  | Stage name to start from when mode is `from` or `single`                       |
| `skip-clarify`        | No       | boolean | Skip the interactive clarification stage (default: false)                      |

## Pipeline Overview

The feature specification pipeline consists of 7 stages executed in strict order:

```
Stage 1: Specify  ──→  Stage 2: Clarify (optional)  ──→  Stage 3: Plan
                                                                │
                                                                ▼
                  Stage 4: Behavior  ──→  Stage 5: Tasks  ──→  Stage 6: Adversarial Analyze
                                                                       │
                                                                       ▼
                                                              Stage 7: Operator Review
                                                                       │
                                                                       ▼
                                                                  Handoff to
                                                             build-orchestrator
                                                              or taskstoissues
```

Context compaction occurs after Stages 1, 3, 4, 6, and 7 — the stages that produce the largest artifacts and consume the most context window space.

## Required Steps

### Step 1: Resolve Pipeline Scope

Determine which stages to execute based on the `mode` parameter:

**Full mode** (default):
* Build the stage queue: `[specify, clarify, plan, behavior, tasks, analyze, operator-review]`
* If `skip-clarify` is true, remove `clarify` from the queue.

**From mode**:
* Validate that `start-stage` is provided and is a valid stage name.
* Build the stage queue starting from `start-stage` through `operator-review`.
* If `start-stage` is after `clarify`, or `skip-clarify` is true, exclude `clarify`.
* Validate that prerequisite artifacts exist for the starting stage (see Stage Entry Gates below).

**Single mode**:
* Validate that `start-stage` is provided and is a valid stage name.
* Build the stage queue containing only `[start-stage]`.
* Validate that prerequisite artifacts exist for the stage.

Report the resolved stage queue to the user and proceed.

### Step 2: Pipeline Execution Loop

For each stage in the queue, execute the following cycle:

1. **Entry gate**: Verify the stage's prerequisite artifacts exist (see Stage Entry Gates).
2. **Dispatch**: Execute the stage by following the corresponding speckit agent's instructions (see Stage Dispatch Reference).
3. **Exit gate**: Verify the stage produced its expected output artifacts (see Stage Exit Gates).
4. **Compact** (if applicable): Run the compact-context skill at `.github/skills/compact-context/SKILL.md` after designated stages.
5. **Advance**: Move to the next stage in the queue.

If any stage fails its exit gate:
* Report the failure with specific missing artifacts or validation errors.
* Attempt remediation by re-running the stage once.
* If the retry also fails, halt the pipeline and report the blocking issue.

### Step 3: Stage Dispatch Reference

Each stage is executed by reading and following the corresponding agent file's instructions. The agent files contain the complete workflow for each stage — do not duplicate or override their logic. Instead, invoke them as described below.

#### Stage 1: Specify

**Agent**: `.github/agents/speckit.specify.agent.md`  
**Purpose**: Generate the feature specification (`spec.md`), create the feature branch, and produce initial checklists.
**Dispatch**: Read the specify agent and follow its complete workflow, passing `feature-description` as the user input ($ARGUMENTS). This stage creates the feature branch and `specs/###-feature-name/` directory.
**Context to pass**: The full `feature-description` text.
**Compact after**: Yes — the specify stage loads templates and generates substantial spec content.

#### Stage 2: Clarify (Optional)

**Agent**: `.github/agents/speckit.clarify.agent.md`
**Purpose**: Reduce ambiguity in the spec through up to 5 targeted clarification questions.
**Dispatch**: Read the clarify agent and follow its workflow. This stage is interactive — it asks the user questions and encodes answers back into the spec.
**Skip conditions**: Skipped when `skip-clarify` is true, or when the user explicitly states they want to skip clarification.
**Context to pass**: None beyond what the agent discovers from the spec file.

> **Note**: This is the only interactive stage. All other stages run autonomously.

#### Stage 3: Plan

**Agent**: `.github/agents/speckit.plan.agent.md`
**Purpose**: Generate the implementation plan (`plan.md`), research document (`research.md`), data model (`data-model.md`), API contracts (`contracts/`), and quickstart guide (`quickstart.md`).
**Dispatch**: Read the plan agent and follow its complete workflow including Phase 0 (research) and Phase 1 (design & contracts). The plan agent also runs `update-agent-context.ps1` to update agent context files.
**Context to pass**: None beyond what the agent discovers from scripts and spec.
**Compact after**: Yes — the plan stage produces multiple large artifacts and consumes significant context.

#### Stage 4: Behavior

**Agent**: `.github/agents/speckit.behavior.agent.md`
**Purpose**: Generate the behavioral matrix (`SCENARIOS.md`) mapping all permutations, edge cases, and expected outcomes.
**Dispatch**: Read the behavior agent and follow its complete workflow. This stage reads the spec and plan, then generates a comprehensive scenario table.
**Context to pass**: None beyond what the agent discovers from the feature directory.
**Compact after**: Yes — the behavior matrix can be substantial and the accumulated context from prior stages needs reclaiming.

> **Important**: The behavior stage is **mandatory** in this orchestration. It must not be skipped, as `SCENARIOS.md` is the authoritative source for test scenarios in downstream stages.

#### Stage 5: Tasks

**Agent**: `.github/agents/speckit.tasks.agent.md`
**Purpose**: Generate the phased task breakdown (`tasks.md`) organized by user story with dependency ordering.
**Dispatch**: Read the tasks agent and follow its complete workflow. If `SCENARIOS.md` exists (from Stage 5), the tasks agent uses it as the authoritative source for test scenarios.
**Context to pass**: None beyond what the agent discovers from the feature directory.

#### Stage 6: Adversarial Analyze

**Agent**: `.github/agents/speckit.analyze_adversarial.agent.md`
**Purpose**: Perform adversarial multi-model analysis with automated remediation across spec.md, plan.md, tasks.md, and SCENARIOS.md. Unlike the read-only `speckit.analyze` agent, this stage synthesizes findings from three independent model reviewers (Claude Opus 4.6, GPT-5.3 Codex, Gemini 3.1 Pro Preview) and applies critical/high fixes directly.
**Dispatch**: Read the adversarial analyze agent and follow its complete workflow. This stage dispatches three parallel adversarial reviewer subagents, synthesizes their findings, and applies severity-gated remediation (critical/high auto-applied, medium presented to user, low recorded as suggestions).
**Context to pass**: None beyond what the agent discovers from the feature directory and constitution.
**Compact after**: Yes — the adversarial analysis produces substantial review artifacts and accumulated context needs reclaiming before operator review.

> **Important**: This stage replaces the standard `speckit.analyze` agent with the adversarial multi-model analyzer. The adversarial approach provides stronger coverage through independent review perspectives and automated severity-based remediation.

#### Stage 7: Operator Review (Mandatory)

**Agent**: Inline — executed directly by the orchestrator skill (no separate agent file).
**Purpose**: Communicate each adversarial review finding to the remote operator via agent-intercom, collect approval or refinement feedback for each finding, and apply the operator-approved spec modifications.
**Dispatch**: Execute the Operator Review Protocol defined below.
**Context to pass**: The unified findings from Stage 6, the current state of all spec artifacts.
**Compact after**: Yes — final compaction before handoff ensures a clean context state.

> **Important**: This stage is **mandatory**. Every finding from the adversarial analysis must be communicated to the remote operator before spec modifications are finalized. The operator has final authority over all specification changes.

##### Operator Review Protocol

This protocol uses the agent-intercom MCP server to present adversarial review findings to the remote operator and collect actionable feedback.

**1. Detect agent-intercom availability**:
* Call `ping` with `status_message: "Operator review starting for adversarial analysis findings"`. If the call succeeds, agent-intercom is active. If it fails, halt and report that the operator review stage requires agent-intercom to be running — do not skip this stage.

**2. Broadcast review session start**:
* Call `broadcast` at `info` level: `[📋 SPEC REVIEW] Starting operator review — {finding_count} findings from adversarial analysis`.
* Capture the returned `ts` and use it as `thread_ts` for all subsequent messages in this review session.

**3. Present findings one-by-one**:

For each finding from the Stage 6 unified findings table (ordered by severity: critical → high → medium → low):

* Call `transmit` with the following parameters:
  * `prompt_type`: `"approval"`
  * `title`: `"[{severity}] {finding_id}: {summary}"`
  * `body`: A formatted message containing:
    * **Finding ID**: The finding identifier (e.g., RC-01, TF-03, ES-07)
    * **Severity**: CRITICAL, HIGH, MEDIUM, or LOW
    * **Consensus**: unanimous, majority, or single
    * **Location**: File and section/line reference
    * **Summary**: The finding description
    * **Recommended Fix**: The proposed remediation
    * **Affected Artifact**: Which spec file (spec.md, plan.md, tasks.md, SCENARIOS.md)
  * `options`: Present the operator with these choices:
    * `approve` — Apply the recommended fix as-is
    * `modify` — Apply with modifications (operator provides revised fix text)
    * `defer` — Record the finding but do not apply now (will appear in the final report as deferred)
    * `reject` — Dismiss the finding entirely (operator believes it is incorrect or not applicable)

* The `transmit` call **blocks** until the operator responds. Capture the operator's choice and any accompanying text.

* `broadcast` the operator's decision at `info` level in the review thread: `[📋 SPEC REVIEW] {finding_id}: {operator_choice}` (include the operator's modification text if they chose `modify`).

**4. Apply operator-approved changes**:

After collecting responses for all findings, apply the approved modifications:

* For each finding where the operator chose `approve`:
  * Apply the recommended fix to the affected spec artifact.
  * Record the change in the remediation log.

* For each finding where the operator chose `modify`:
  * Apply the operator's revised fix text to the affected spec artifact.
  * Record the original recommendation and the operator's modification in the remediation log.

* For each finding where the operator chose `defer`:
  * Do not modify any artifact.
  * Record in the remediation log as deferred with the operator's reason (if provided).

* For each finding where the operator chose `reject`:
  * Do not modify any artifact.
  * Record in the remediation log as rejected with the operator's reason (if provided).

* When modifying spec artifacts, preserve their structural format. Do not reorganize sections, renumber existing items, or alter content unrelated to the finding being remediated.

**5. Verification pass**:

After all approved changes are applied:
* Re-read the modified artifacts.
* Run the `.specify/scripts/powershell/check-prerequisites.ps1 -Json -RequireTasks -IncludeTasks` script to verify all artifacts remain well-formed.
* If any artifact is malformed after modifications, report the issue and attempt a corrective edit. If correction fails, halt and report to the operator via `transmit` with `prompt_type: "error_recovery"`.

**6. Produce operator review summary**:

`broadcast` the final summary at `success` level in the review thread:

```
[📋 SPEC REVIEW] Operator review complete:
• Approved: {count} findings applied
• Modified: {count} findings applied with operator revisions
• Deferred: {count} findings recorded for future consideration
• Rejected: {count} findings dismissed
• Artifacts modified: {list of changed files}
```

**7. Record remediation log**:

Write the operator review remediation log to `specs/###-feature-name/operator-review-log.md` with:
* Date and time of the review session
* Total findings reviewed
* Per-finding decision table: Finding ID, Severity, Consensus, Operator Decision, Modification Notes
* List of artifacts modified with change descriptions
* List of deferred findings with operator reasons
* List of rejected findings with operator reasons

### Step 4: Stage Entry Gates

Before executing each stage, verify its prerequisites:

| Stage         | Required Artifacts                                                                 |
| ------------- | ---------------------------------------------------------------------------------- |
| Specify       | `.specify/templates/spec-template.md` and `.specify/memory/constitution.md` exist  |
| Clarify       | `specs/###-feature-name/spec.md` exists                                            |
| Plan          | `specs/###-feature-name/spec.md` exists                                            |
| Behavior      | `specs/###-feature-name/spec.md` and `plan.md` exist                               |
| Tasks         | `specs/###-feature-name/plan.md` and `spec.md` exist                               |
| Analyze       | `specs/###-feature-name/spec.md`, `plan.md`, and `tasks.md` exist                  |
| Operator Review | `specs/###-feature-name/spec.md`, `plan.md`, `tasks.md` exist; adversarial analysis report from Stage 6 is available |

If an entry gate fails:
* In `full` mode: This indicates a prior stage failed silently. Report the error and halt.
* In `from` or `single` mode: Report which prerequisite artifacts are missing and suggest running the appropriate earlier stage first.

### Step 5: Stage Exit Gates

After each stage completes, verify its outputs:

| Stage         | Required Outputs                                                                   | Validation                        |
| ------------- | ---------------------------------------------------------------------------------- | --------------------------------- |
| Specify       | `specs/###-feature-name/spec.md`                                                   | File exists, contains FR- entries |
| Clarify       | `specs/###-feature-name/spec.md` (updated)                                         | File modified timestamp changed   |
| Plan          | `plan.md`, `research.md` in feature dir                                            | Files exist and are non-empty     |
| Behavior      | `SCENARIOS.md` in feature dir                                                      | File exists, contains S001+       |
| Tasks         | `tasks.md` in feature dir                                                          | File exists, contains T001+       |
| Analyze       | Analysis report output (displayed, not necessarily a file)                         | Report was generated              |
| Operator Review | `specs/###-feature-name/operator-review-log.md`                                  | File exists, contains per-finding decisions |

### Step 6: Context Compaction

Context compaction is performed after Stages 1, 3, 4, 6, and 7 by reading and following the compact-context skill at `.github/skills/compact-context/SKILL.md`.

**Compaction protocol**:
1. Read and follow the compact-context skill's complete workflow (Steps 1–4).
2. The skill creates a checkpoint file at `.copilot-tracking/checkpoints/{YYYY-MM-DD}-{HHmm}-checkpoint.md`.
3. After compaction, verify the checkpoint file was created.
4. Continue to the next stage — the checkpoint ensures recovery is possible if the session must restart.

**Recovery from compaction**:
If the session is restarted after compaction (e.g., new chat window), the pipeline can be resumed:
1. Read the most recent checkpoint file in `.copilot-tracking/checkpoints/`.
2. Identify the last completed stage from the checkpoint's Task State or Session Summary.
3. Invoke this skill with `mode: from` and `start-stage` set to the next stage after the last completed one.

### Step 7: Pipeline Completion and Handoff

When all stages in the queue have completed successfully:

1. **Generate completion summary**:
   * List all stages completed with their output artifacts and paths.
   * Report the feature spec directory path.
   * Report the feature branch name.
   * Summarize key specification decisions (from spec.md and plan.md).
   * Report the total scenario count from SCENARIOS.md.
   * Report the total task count and phase count from tasks.md.

2. **Present handoff options**:
   The specification lifecycle is complete. Present the user with next-step options:

   > **Specification Complete.** All SDD stages have been completed successfully for this feature.
   >
   > **Your specification artifacts are ready at**: `specs/###-feature-name/`
   >
   > Your next steps:
   > - **Build the feature**: Invoke the build-orchestrator to implement the tasks phase by phase.
   > - **Create GitHub issues**: Run `/speckit.taskstoissues` to convert tasks into trackable GitHub issues.
   > - **Review artifacts**: Manually review the generated spec, plan, scenarios, and tasks before proceeding.

3. **Do NOT automatically start the build-orchestrator.** The build lifecycle is a separate concern and must be started independently by the user.

## Stage Transition Reporting

Between each stage, report a brief transition summary:

```markdown
### Stage {N} Complete: {Stage Name}

**Produced**: {list of artifacts with paths}
**Next**: Stage {N+1}: {Next Stage Name}
**Compacting**: {Yes/No}
```

## Error Handling

| Error Type                     | Action                                                                |
| ------------------------------ | --------------------------------------------------------------------- |
| Missing prerequisite script    | Halt pipeline, report which `.specify/scripts/` file is missing       |
| Agent file not found           | Halt pipeline, report which `.github/agents/` file is missing         |
| Stage produces no output       | Retry stage once; if second attempt fails, halt with diagnostics      |
| Entry gate failure (full mode) | Halt — indicates a silent failure in a prior stage                    |
| Entry gate failure (from mode) | Report missing artifacts, suggest running prerequisite stages         |
| Compaction failure              | Warn but continue — compaction is best-effort, not a hard gate       |
| Git operation failure          | Report the git error, suggest manual resolution, halt                 |
| agent-intercom unavailable     | Halt at Stage 7 — operator review requires agent-intercom to be running. Report the error and instruct the user to start the MCP server before retrying with `from operator-review`. |
| Operator review timeout        | If operator does not respond within the configured timeout, `broadcast` a reminder. If no response after two reminders, halt and report.  |

## How It Works

The spec-kit workflow is a linear pipeline where each stage builds on the artifacts produced by prior stages. Without orchestration, the user must manually invoke each stage via `/speckit.{stage}` commands and manage context window pressure themselves.

This skill automates the pipeline by:
1. **Sequencing** — ensuring stages execute in the correct dependency order.
2. **Validating** — checking entry and exit gates so no stage runs against missing prerequisites.
3. **Compacting** — invoking the compact-context skill at strategic points to prevent context window exhaustion across the multi-stage workflow.
4. **Recovering** — enabling pipeline resumption from any stage via checkpoint files.
5. **Reviewing** — communicating adversarial analysis findings to the remote operator via agent-intercom for approval before applying spec modifications.
6. **Handing off** — clearly separating the specification lifecycle from the implementation lifecycle (build-orchestrator).

The pipeline treats the behavioral matrix (`SCENARIOS.md` from Stage 4) as mandatory because it serves as the authoritative source for test scenarios during implementation — ensuring that the specification is rigorous enough to drive TDD.

The pipeline also treats operator review (Stage 7) as mandatory because adversarial analysis findings must be vetted by the remote operator before spec modifications are considered final. This ensures human oversight of automated analysis recommendations and prevents false positives from corrupting the specification artifacts.

## Troubleshooting

### Pipeline stalls after compaction

If the context window was fully reset and the pipeline cannot continue:
1. Read the latest checkpoint file in `.copilot-tracking/checkpoints/`.
2. Re-invoke the skill with `mode: from` and `start-stage` set to the next incomplete stage.
3. The stage entry gates will verify all prerequisite artifacts are in place before resuming.

### Stage retry fails twice

If a stage fails on both initial execution and retry:
1. Check the terminal output for specific error messages.
2. Verify the prerequisite artifacts are well-formed (not empty, contain expected content).
3. Run the stage manually via its `/speckit.{stage}` command for interactive debugging.
4. Once resolved, resume the pipeline with `mode: from`.

### Feature directory not found after specify stage

The specify stage creates the feature branch and directory. If the directory is missing:
1. Verify the feature branch was created: `git branch --list *feature-name*`
2. Check if the branch was checked out: `git branch --show-current`
3. Look for the spec directory: `ls specs/`
4. If missing, re-run the specify stage manually: `/speckit.specify {feature-description}`
5. If the issue persists, check the specify agent's instructions and ensure it is correctly creating branches and directories.
