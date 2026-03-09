---
name: compact-context
description: "Usage: Compact context. Captures the current session state into a structured checkpoint file, then compacts the conversation history to reclaim context window space."
version: 1.0
---

# Compact Context Skill

Captures the current session state into a structured checkpoint file and compacts the conversation history. Use this skill when the context window is approaching its limit or when you want to preserve session continuity before a long operation.

## Prerequisites

* The workspace root contains a `.copilot-tracking/` directory (created automatically if missing).

## Quick Start

Invoke the skill:

```text
Compact context
```

The skill runs autonomously through all required steps, producing a checkpoint file and compacting the conversation.

## Parameters Reference

| Parameter | Required | Type   | Description                                                    |
| --------- | -------- | ------ | -------------------------------------------------------------- |
| *none*    | —        | —      | This skill takes no parameters. It infers state from context.  |

## Required Steps

### Step 1: Gather Session State

Analyze the current session to identify:

* **Active tasks**: Any in-progress or recently completed work from the todo list.
* **Files read**: List of files loaded into context during this session (source, tests, configs, docs).
* **Files modified**: List of files created or edited during this session, with a one-line summary of each change.
* **Key decisions**: Architectural or implementation decisions made during this session.
* **Failed approaches**: Approaches attempted and abandoned, with the reason for abandonment.
* **Open questions**: Unresolved questions or ambiguities that remain.
* **Current working directory**: The active directory context.
* **Active branch**: The current Git branch if applicable.

Do not re-read files to gather this information. Reconstruct it from the conversation history and tool call results already in context.

### Step 2: Write Checkpoint File

Create a checkpoint file at:

```text
.copilot-tracking/checkpoints/{YYYY-MM-DD}-{HHmm}-checkpoint.md
```

Where `{YYYY-MM-DD}` is today's date and `{HHmm}` is the current time (24-hour, zero-padded).

Use this template:

```markdown
# Session Checkpoint

**Created**: {YYYY-MM-DD} {HH:mm}
**Branch**: {branch-name or "N/A"}
**Working Directory**: {cwd}

## Task State

{If a todo list is active, reproduce it here with current statuses.
If no todo list, write "No active task list."}

## Session Summary

{2-4 sentence summary of what was accomplished in this session so far.}

## Files Modified

{Bulleted list of files modified with one-line change descriptions.
If none, write "No files modified."}

| File | Change |
| ---- | ------ |
| path/to/file.rs | Added streaming iterator for row deduplication |

## Files in Context

{Bulleted list of key files read during the session that would be needed
to continue the work. Limit to the 15 most relevant files.}

## Key Decisions

{Numbered list of decisions made and their rationale.
If none, write "No significant decisions recorded."}

## Failed Approaches

{Bulleted list of approaches tried and abandoned, with reason.
If none, write "No failed approaches."}

## Open Questions

{Bulleted list of unresolved questions.
If none, write "No open questions."}

## Next Steps

{What should happen next to continue this work. Include specific file
paths, function names, or task references where possible.}

## Recovery Instructions

To continue this session's work, read this checkpoint file and the
following resources:

- This checkpoint: .copilot-tracking/checkpoints/{this-file}
- {List any other files critical for resumption: specs, schemas, etc.}
```

### Step 3: Report Checkpoint

Report to the user:

* The checkpoint file path.
* A one-line summary of what was captured.
* The estimated token reduction expected from compaction.

### Step 4: Compact Conversation History

Compact the current conversation to reclaim context window space.

Run the `/compact` command to compact the conversation history.

If `/compact` is not available in the current environment, inform the user that automatic compaction is not supported and recommend they:

1. Start a new chat session.
2. Begin the new session by reading the checkpoint file created in Step 2.

## How It Works

The context window has a fixed size. As a session progresses, earlier context gets pushed out or truncated. This skill mitigates that by:

1. **Persisting** the important session state to a file on disk before it gets lost.
2. **Compacting** the conversation so the agent regains working space.
3. **Enabling recovery** by providing a structured file that a new or compacted session can load to restore continuity.

The checkpoint file acts as durable memory. Even if the context window is fully reset, reading the checkpoint brings back the essential state without replaying the entire session.

## Troubleshooting

### Checkpoint directory does not exist

The skill creates `.copilot-tracking/checkpoints/` automatically. If permission errors occur, create the directory manually:

```bash
mkdir -p .copilot-tracking/checkpoints
```

### /compact is not available

The `/compact` command depends on the VS Code Copilot Chat version. If unavailable:

* Start a new chat session and reference the checkpoint file.
* Or continue working in the current session — the checkpoint is still saved for future recovery.

### Checkpoint file is too large

If the checkpoint exceeds 200 lines, trim the Files in Context section to the 10 most critical files and condense the Session Summary.
