//! Build script for graphtor-core.
//!
//! On Windows, explicitly links `advapi32` which is required by `libgit2-sys`
//! but is not always propagated to integration test binaries by cargo.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
