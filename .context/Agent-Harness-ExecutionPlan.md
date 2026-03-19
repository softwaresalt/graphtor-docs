# **Agent-Harness & Beads Implementation Plan**

**Objective:** Transition the workspace from a Spec-Driven Development (SDD) pattern to a strict Agent-Harness pattern. The local agent must execute the following phases to eliminate obsolete markdown generation, establish true compiling test harnesses as the single source of truth, and rewire the orchestration to loop mechanistically against the Rust compiler.

This paradigm shift resolves the inherent fragility of maintaining synchronization between dense, static text documents and dynamic executable code. By relying on the Rust compiler as the ultimate arbiter of truth, we drastically reduce token overhead, eliminate LLM hallucinations during implementation review, and guarantee that architectural intent translates directly and definitively into verified behavior.

## **Phase 1: The Purge (Removing SDD Artifacts)**

The first step is to aggressively remove the legacy SpecKit scaffolding. We are moving away from "Markdown constraints"—which require constant upkeep and are prone to semantic drift—to immutable "Compiler constraints."

**Agent Directives:**

1. **Delete** the following template files as they are no longer used to generate static plans. Leaving these legacy files in the repository risks creating a "split brain" scenario where execution agents might accidentally reference outdated, flat markdown plans instead of the live, state-driven Beads database:  
   * .specify/templates/tasks-template.md  
   * .specify/templates/plan-template.md  
   * .specify/templates/spec-template.md  
   * .specify/templates/scenarios-template.md  
2. **Delete** any legacy SpecKit agents that focus purely on Markdown generation (e.g., speckit.specify.agent.md, speckit.plan.agent.md, speckit.tasks.agent.md). Their roles are now obsolete.  
3. **Verify** that .github/agents/harness-architect.agent.md is active and set as the primary entry point for backlog analysis and harness construction.

## **Phase 2: Implementing the True Agent-Harness**

The local agent must create the new foundational template that supports the true Agent-Harness methodology. This ensures that integration tests actually compile against structural stubs before any worker agent is dispatched.

### **Pillar 1 & Pillar 4: Embedded Intent & Test-Driven Boundaries**

We completely replace standalone spec.md and SCENARIOS.md files with executable BDD (Behavior-Driven Development) Rust tests **and** their corresponding source code stubs. This bridges the gap between human-readable requirements and machine-verifiable execution.

**Action:** Create .engram/templates/build-harness.prompt.md with the following exact content:

\# Build Harness Generation Prompt

\*\*Role:\*\* You are the execution arm of the Harness Architect. Your output is STRICTLY executable Rust code consisting of test files and structural stubs. No markdown explanations or theoretical architecture documents are permitted.

\*\*Goal:\*\* Translate the finalized architectural constraints into a compiling but failing BDD integration test harness.

\*\*Rules for the Optimal Harness:\*\*  
1\. \*\*The Contract (The Tests):\*\* Generate the integration test file (e.g., \`tests/integration/{feature}\_test.rs\`). Every test function must represent one specific scenario. Use \`// GIVEN\`, \`// WHEN\`, and \`// THEN\` inside the function to explicitly define the intent. This BDD commentary replaces the need for separate scenario matrices by tightly coupling behavioral requirements directly to the test logic.  
2\. \*\*The Boundary (The Stubs):\*\* You MUST also generate the corresponding structural stubs in the \`src/\` directory (e.g., \`src/{feature}.rs\`). You must define the exact \`struct\`, \`enum\`, and \`trait\` signatures required for the test to compile. This guarantees that the architectural skeleton is sound and that the test files have a valid API surface to interface with.  
3\. \*\*The Red Phase:\*\* Do NOT implement the core logic in the \`src/\` files. The bodies of the functions in the \`src/\` stubs MUST contain \`unimplemented\!("Worker: \[Specific implementation instructions\]")\`. This explicitly marks the starting line for the worker agent. By ensuring the test compiles structurally but fails logically at runtime with a clear panic message, the worker knows exactly which boundaries it operates within.  
4\. \*\*Beads Registration:\*\* Output the CLI commands to seamlessly register this harness in the Beads state machine:  
   \`bd create \--title "Implement {Feature}: {Test}" \--harness "cargo test \--test {feature}\_test \-- {test\_name}"\`

### **Pillar 2: Context Isolation (Agent-Engram)**

A worker agent must not read or grep the entire repository, as doing so bloats the context window and invites hallucination. It will use Agent-Engram to map *only* the specific structural files touched by the failing harness. Instead of searching blindly, the worker leverages semantic mapping to understand the exact blast radius of its task, ensuring highly focused and localized code generation. (This is wired directly into the SKILL.md update below).

## **Phase 3: Rewiring the Execution Engine**

The local agent must modify the Orchestrator and the primary Execution Skill to implement **Pillar 3 (Mechanical Feedback)** and **Pillar 5 (State-Driven Progression)**.

### **Action 1: Update .github/agents/build-orchestrator.agent.md**

Replace the entire \#\# Inputs and \#\# Execution Loop sections. By removing the brittle, phase-based markdown logic, we delegate all dependency management and task sequencing directly to the Beads database.

\#\# Inputs

\* \`${input:mode:single}\`: (Optional, defaults to \`single\`) Execution mode:  
  \* \`single\` — Claim one unblocked task from Beads, build its harness, and stop execution.  
  \* \`drain\` — Loop sequentially through all unblocked, active tasks in the Beads \`ready\` queue until the queue is completely empty.

\#\# Execution Loop

\#\#\# Step 1: Check Queue (State-Driven Progression)  
Run \`bd ready \--json\`. If the queue is empty, exit immediately and report completion to the user. The state machine dictates when work is finished.

\#\#\# Step 2: Claim & Delegate  
1\. Select the top task from the \`bd ready\` output based on priority.  
2\. Claim it: \`bd update \<task\_id\> \--claim\` to lock the task from other agents.  
3\. Extract the \`--harness\` command from the Beads payload (e.g., \`cargo test \--test feature\_test\`).  
4\. Delegate execution to \`.github/skills/build-feature/SKILL.md\`, passing the \`task-id\` and \`harness-cmd\`.

\#\#\# Step 3: Loop or Exit  
If \`${input:mode}\` is \`drain\`, return to Step 1 and evaluate the next unblocked item.

### **Action 2: Update .github/skills/build-feature/SKILL.md**

Rewrite the build skill to execute a mechanical, compiler-driven Actor-Critic loop, effectively discarding the need for an LLM reviewer.

Replace the input: block and \# Build Feature Skill sections:

input:  
  properties:  
    task-id:  
      type: string  
      description: "The unique Beads task ID."  
    harness-cmd:  
      type: string  
      description: "The cargo test command defining the strict compiler harness boundary."  
  required:  
    \- task-id  
    \- harness-cmd  
\---

\# Build Feature Skill

Implements a requested feature by continuously looping a fast worker agent against a strict, compiling, but failing test harness until success is achieved.

\#\# Execution Steps

\#\#\# Step 1: Context Isolation  
1\. Read the test file targeted by the \`${input:harness-cmd}\`. Carefully read the embedded \`// GIVEN\`, \`// WHEN\`, \`// THEN\` BDD comments to fully internalize the human intent behind the test.  
2\. Call the \`agent-engram\` MCP tool (e.g., \`map\_code\`) using the domain structs and functions found in the test. This will map the exact source files in \`src/\` containing the \`unimplemented\!()\` stubs that require your attention.

\#\#\# Step 2: Mechanical Feedback Loop (Actor-Critic)  
1\. Run the targeted \`${input:harness-cmd}\`.  
2\. If it fails (exit code \!= 0), capture the raw \`stderr\` (compiler errors, type mismatches, or panic traces).  
3\. Pipe the \`stderr\` directly back to \`rust-engineer.agent.md\` with the explicit instruction: \*"Implement the underlying logic inside the \`src/\` stubs to make this harness pass. Replace the \`unimplemented\!()\` macros. Do not modify the test file itself unless fixing a compilation error in the test setup."\*  
4\. Repeat this step iteratively until \`${input:harness-cmd}\` passes. \*\*Hard limit: 5 attempts.\*\* This strict circuit breaker prevents fast models from entering infinite hallucination loops and burning token budgets. If it fails 5 times, run \`bd update ${input:task-id} \--status blocked\` and halt execution for human review.

\#\#\# Step 3: Verification & State Update  
1\. Once the isolated harness passes, verify no existing peripheral tests were broken by running \`cargo test \--workspace\`.  
2\. Commit the validated changes: \`git commit \-am "feat: implement passing harness for ${input:task-id}"\`.  
3\. Mechanically mark the task complete in Beads: \`bd update ${input:task-id} \--status done\`.

## **Final Verification Checklist for Local Agent**

* ![][image1]Legacy SpecKit markdown document templates have been successfully deleted from the workspace.  
* ![][image1]The .engram/templates/build-harness.prompt.md file is created and strictly enforces Structural Stub Generation alongside BDD testing rules.  
* ![][image1]build-orchestrator.agent.md has been refactored to remove phase parsing, relying solely on bd ready for task orchestration.  
* ![][image1]build-feature/SKILL.md is updated to implement the pure, structural cargo test loop with standard error pipeline feedback.  
* ![][image1]Confirm the harness-architect.agent.md persona is present, active, and correctly references the new build-harness prompt template for all backlog synthesis.

[image1]: <data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAmwAAAAUCAYAAAAwe2GgAAAAR0lEQVR4Xu3BMQEAAADCoPVPbQlPoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAP4GwdQAAZuLbQIAAAAASUVORK5CYII=>