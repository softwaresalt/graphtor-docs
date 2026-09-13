#![forbid(unsafe_code)]
//! Thin composition root for the standalone, non-published `mcp-probe`
//! diagnostic crate (056-F / 049-S: MCP serve initialize-handshake
//! regression investigation).
//!
//! This crate is never installed, never committed as a shipped binary, and
//! is invalid as production (T4) acceptance evidence -- see
//! `tools/mcp-probe/Cargo.toml`.
//!
//! `056.020-T` owns only the core transport (exposed via the `mcp_probe`
//! library target's `transport` module -- see `src/lib.rs`) and this thin
//! entry point, plus the hidden, in-crate, platform-portable self-test
//! helper modes below (`__echo` / `__block` / `__exit`). These helper
//! modes are consumed by this crate's own black-box integration
//! self-tests (`tools/mcp-probe/tests/`) via
//! `env!("CARGO_BIN_EXE_mcp-probe")`, re-exec'ing this exact binary rather
//! than an external OS-specific helper. `056.022-T` adds the versioned
//! `wrapper` subcommand (composing process spawning/teardown onto the
//! `056.020-T` transport -- see `src/process.rs`). `056.021-T` adds the
//! isolated `logs/probe/<nonce>` workspace and control/treatment/ancestor
//! `.mcp.json` fixture composition (`mcp_probe::workspace` -- see
//! `src/workspace.rs`); it performs no production acceptance and adds no
//! subcommand of its own -- `056.001-T`'s forthcoming `exact-cli`
//! subcommand is the sole caller that composes `workspace::create_probe_workspace`
//! with the `wrapper` subcommand above into one real run.

use mcp_probe::exact_cli::{
    outcome_to_json, parse_exact_cli_args, persist_outcome_json, run_exact_cli,
};
use mcp_probe::process::{parse_wrapper_args, run_wrapper, SysinfoProcessObserver, WrapperConfig};
use std::io::{Read, Write};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("__echo") => run_echo_child(),
        Some("__block") => run_block_child(),
        Some("__exit") => run_exit_child(args),
        Some("wrapper") => run_wrapper_subcommand(args),
        Some("exact-cli") => run_exact_cli_subcommand(args),
        Some(other) => {
            eprintln!("mcp-probe: unknown subcommand '{other}'");
            std::process::exit(2);
        }
        None => {
            eprintln!(
                "mcp-probe: standalone, non-published diagnostic probe for the \
                 056-F MCP serve initialize-handshake regression investigation. \
                 Subcommands: wrapper (056.022-T), exact-cli (056.001-T)."
            );
            std::process::exit(2);
        }
    }
}

/// Composes process spawning/teardown and the `056.020-T` transport for
/// the versioned `wrapper` subcommand. Argv contract: `--inner-exe`,
/// repeated `--inner-arg`, `--evidence-output`, `--run-nonce`. Preserves
/// the inner child's exit code, stderr, half-close, and deadline
/// behavior unchanged; see `mcp_probe::process` for the full contract.
/// This subcommand persists no evidence itself -- serializing to
/// `--evidence-output` is `056.023-T`'s job.
fn run_wrapper_subcommand(args: impl Iterator<Item = String>) {
    let parsed = match parse_wrapper_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("mcp-probe wrapper: {message}");
            std::process::exit(2);
        }
    };

    let config = WrapperConfig {
        args: parsed,
        // Production wrapper runs never bound the pump wait themselves;
        // the outer timeout belongs to 056.001-T's exact-CLI runner. Only
        // this crate's own integration self-tests construct a
        // `WrapperConfig` directly with a bounded deadline.
        pump_deadline: None,
    };
    let observer = SysinfoProcessObserver::new();

    match run_wrapper(std::io::stdin(), std::io::stdout(), &config, &observer) {
        Ok(outcome) => std::process::exit(outcome.inner_exit_code.unwrap_or(1)),
        Err(err) => {
            eprintln!("mcp-probe wrapper: failed to run inner process: {err}");
            std::process::exit(1);
        }
    }
}

/// Composes the isolated `056.021-T` probe workspace, the `056.022-T`
/// process guards/wrapper, and the `056.023-T` evidence seam for the
/// `exact-cli` subcommand (`056.001-T`). Prints the full structured
/// classification outcome as JSON to stdout and exits `0` on any
/// completed run (including a Gate-1-fail `H3-B-candidate` terminal,
/// which is a normal `done` outcome, never a failure); exits non-zero
/// only when argument parsing fails or evidence capture itself could not
/// even begin (for example, the isolated workspace could not be
/// created).
fn run_exact_cli_subcommand(args: impl Iterator<Item = String>) {
    let parsed = match parse_exact_cli_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("mcp-probe exact-cli: {message}");
            std::process::exit(2);
        }
    };

    match run_exact_cli(&parsed) {
        Ok(outcome) => {
            if let Err(err) = persist_outcome_json(&outcome) {
                eprintln!(
                    "mcp-probe exact-cli: warning: failed to persist result JSON under the \
                     probe workspace: {err}"
                );
            }
            let json = outcome_to_json(&outcome);
            match serde_json::to_string_pretty(&json) {
                Ok(text) => println!("{text}"),
                Err(err) => {
                    eprintln!("mcp-probe exact-cli: failed to serialize outcome: {err}");
                    std::process::exit(1);
                }
            }
        }
        Err(message) => {
            eprintln!("mcp-probe exact-cli: {message}");
            std::process::exit(1);
        }
    }
}

/// Hidden, in-crate, platform-portable self-test helper: a std-only
/// echo/sink child used ONLY by `transport`'s own self-tests via a
/// `std::env::current_exe` re-exec, standing in for the inner MCP server
/// during transport-only self-tests. Never used for anything other than
/// this crate's own test fixtures.
///
/// When the `MCP_PROBE_TEST_STDERR_SPAM_LINES` environment variable is set
/// to a positive integer, this helper also writes that many short lines to
/// stderr while echoing, letting self-tests prove the concurrent bounded
/// stderr drain never blocks the primary stdout forwarding.
fn run_echo_child() {
    if let Ok(raw) = std::env::var("MCP_PROBE_TEST_STDERR_SPAM_LINES") {
        if let Ok(lines) = raw.parse::<u32>() {
            let mut stderr = std::io::stderr();
            for i in 0..lines {
                let _ = writeln!(stderr, "mcp-probe test stderr spam line {i}");
            }
        }
    }

    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut buf = [0_u8; 4096];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if stdout.write_all(&buf[..n]).is_err() {
                    break;
                }
                let _ = stdout.flush();
            }
        }
    }
}

/// Hidden, in-crate self-test helper that never reads or writes anything
/// and sleeps well beyond any test deadline, standing in for a wedged
/// child that never produces output or exits on its own. Used ONLY by
/// `transport`'s deadline-signaling self-test and `process`'s
/// deadline-teardown self-test.
fn run_block_child() {
    std::thread::sleep(std::time::Duration::from_secs(3600));
}

/// Hidden, in-crate self-test helper that exits immediately with the
/// exit code given as its sole argument (defaulting to `0` if absent or
/// unparsable), never touching stdio. Used ONLY by `process`'s wrapper
/// self-tests to prove non-zero exit-code preservation deterministically
/// (distinct from `__echo`'s always-`0` exit).
fn run_exit_child(mut args: impl Iterator<Item = String>) {
    let code: i32 = args.next().and_then(|raw| raw.parse().ok()).unwrap_or(0);
    std::process::exit(code);
}
