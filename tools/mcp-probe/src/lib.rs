#![forbid(unsafe_code)]
//! Library surface for the standalone, non-published `mcp-probe`
//! diagnostic crate (056-F / 049-S: MCP serve initialize-handshake
//! regression investigation).
//!
//! This crate is never installed, never published, and never a production
//! dependency -- see `tools/mcp-probe/Cargo.toml`'s empty `[workspace]`
//! table and `publish = false`. The library target exists solely so this
//! crate's own black-box integration self-tests (`tools/mcp-probe/tests/`)
//! can import internal modules directly while separately spawning the
//! actual compiled `mcp-probe` binary as a fixture child process via
//! `env!("CARGO_BIN_EXE_mcp-probe")`.
//!
//! `056.020-T` owns only [`transport`]. Later tasks add further modules on
//! top of it: `056.022-T` owns process spawning/teardown and the `wrapper`
//! subcommand ([`process`]), `056.023-T` owns the observer/evidence seam
//! ([`evidence`]), `056.021-T` owns the isolated probe workspace and
//! control/treatment/ancestor config fixtures ([`workspace`]), and
//! `056.001-T` owns the exact-CLI differential runner.

pub mod evidence;
pub mod process;
pub mod transport;
pub mod workspace;
