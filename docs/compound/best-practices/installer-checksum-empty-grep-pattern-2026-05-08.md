---
problem_type: security-bug
category: installer
component: install-sh
root_cause: empty-grep-match-passes-checksum-check
resolution_type: defensive-coding
severity: high
message: "Piping grep output directly to sha256sum --check can silently pass when no matching line exists"
file_path: install.sh
citations:
  - pr: 44
    sha: 00e3e30
tags: [installer, checksum, sha256, security, shell-script]
---

## Problem

The original checksum verification in `install.sh` piped grep output directly
to `sha256sum --check`:

```sh
grep "${ARCHIVE}" "${SUMS_FILE}" | sha256sum --check --status
```

If `grep` finds no matching line (e.g., the archive name is not in
`SHA256SUMS`), it exits non-zero but the pipeline may still return success
depending on `sha256sum` behavior with empty input. This defeats the integrity
check entirely — a tampered or corrupted download could pass verification.

## Root Cause

`sha256sum --check` reads entries from stdin in the format `<hash>  <file>`.
When fed empty input (no matching grep line), behavior is implementation-
defined. On some systems it exits 0 (success) with no entries checked, making
the verification silently vacuous.

## Resolution

Capture the grep output explicitly, error if empty, then verify:

```sh
EXPECTED_LINE="$(grep "${ARCHIVE}" "${SUMS_FILE}" || true)"
if [ -z "${EXPECTED_LINE}" ]; then
    error "No checksum entry found for ${ARCHIVE} in SHA256SUMS."
fi
printf '%s\n' "${EXPECTED_LINE}" | ${SHASUM_CMD} --check --status \
    || error "Checksum verification failed for ${ARCHIVE}."
```

This ensures:
1. A missing entry in SHA256SUMS is a hard failure, not a silent pass.
2. Only the exact matching line is fed to the checksum tool.
3. The `SHASUM_CMD` variable already resolves the `sha256sum`/`shasum`
   platform difference (set earlier in the script).

## Prevention

- Never pipe grep output directly to checksum tools.
- Always capture the match, validate it is non-empty, then feed it to the
  verifier.
- The same pattern applies to PowerShell: use `@()` array coercion and
  check `.Count` before indexing into the match result.
