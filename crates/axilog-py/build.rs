//! macOS link flags for the `extension-module` build.
//!
//! `pyo3`'s `extension-module` feature deliberately does NOT link
//! libpython: a Python extension module is loaded *into* an interpreter
//! that already provides those symbols, and linking a specific libpython
//! into the `.so` would defeat the stable ABI (`abi3-py39`) and break on
//! any interpreter but the one it was built against. Both features are set
//! in this crate's `Cargo.toml`, on purpose.
//!
//! The consequence is that the cdylib legitimately has undefined
//! `_Py*`/`_PyExc_*` symbols at link time, and the three platforms disagree
//! about whether that is allowed:
//!
//! - **Linux** — `ld` permits undefined symbols in a shared object by
//!   default; they resolve against the interpreter at `dlopen` time.
//! - **Windows** — pyo3 uses `raw-dylib` for the stable ABI, so nothing is
//!   undefined at link time in the first place.
//! - **macOS** — `ld64` errors on undefined symbols unless explicitly told
//!   to defer them, which is what `-undefined dynamic_lookup` does.
//!
//! Build backends paper over this: maturin and setuptools-rust inject the
//! flag themselves, which is why `maturin build` succeeds on macOS while
//! the plain `cargo build --workspace` that CI runs on every leg failed
//! with a wall of `Undefined symbols: _PyExc_TypeError, ...`. This crate is
//! an unconditional workspace member, so a bare `cargo build` has to work
//! on its own.
//!
//! Emitted from a build script rather than a workspace `.cargo/config.toml`
//! so the flag is scoped to THIS crate's link step. The config-file form
//! that pyo3's own macOS guidance suggests sets `rustflags` for the whole
//! target, which would apply these args to every binary in the workspace
//! (the CLI included) — harmless in practice, but far wider than the
//! problem, and it silently overrides any `RUSTFLAGS` the environment sets.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-undefined");
        println!("cargo:rustc-link-arg=dynamic_lookup");
    }
}
