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
//! helper modes below (`__echo` / `__block`). These helper modes are
//! consumed by `mcp_probe::transport`'s own black-box integration
//! self-tests (`tools/mcp-probe/tests/transport_test.rs`) via
//! `env!("CARGO_BIN_EXE_mcp-probe")`, re-exec'ing this exact binary rather
//! than an external OS-specific helper. Later tasks compose additional
//! subcommands onto this same entry point: `056.022-T` adds the versioned
//! `wrapper` subcommand, and `056.001-T` adds the `exact-cli` subcommand.

use std::io::{Read, Write};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("__echo") => run_echo_child(),
        Some("__block") => run_block_child(),
        Some(other) => {
            eprintln!("mcp-probe: unknown subcommand '{other}'");
            std::process::exit(2);
        }
        None => {
            eprintln!(
                "mcp-probe: standalone, non-published diagnostic probe for the \
                 056-F MCP serve initialize-handshake regression investigation. \
                 No production subcommand is wired yet at this point in the \
                 shipment (056.020-T owns only the core transport); see \
                 056.022-T (wrapper) and 056.001-T (exact-cli)."
            );
            std::process::exit(2);
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
/// `transport`'s deadline-signaling self-test.
fn run_block_child() {
    std::thread::sleep(std::time::Duration::from_secs(3600));
}
