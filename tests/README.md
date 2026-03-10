# tests/

Integration tests for `graphtor-core`. Each test file covers one module:

- `config_test.rs` — configuration parsing and validation (Phase 3)
- `error_test.rs` — error type construction and display (Phase 2)
- `chunk_id_test.rs` — chunk ID determinism and format (Phase 4)
- `logging_test.rs` — log initialization and verbosity filtering (Phase 5)
- `path_security_test.rs` — path boundary enforcement (Phase 6)
