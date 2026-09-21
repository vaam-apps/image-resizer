//! Library root for the `emgr` crate.
//!
//! This exists so that criterion benches (`benches/*.rs`) and integration
//! tests (`tests/*.rs`) can link against the pipeline internals
//! (`ImageService`, `CacheService`, `ResizeQuery`, ...) as an ordinary
//! external crate (`emgr::...`), instead of duplicating that logic.
//!
//! Deliberately **not** exported here: `modules` (in particular
//! `modules::api`, owned by other agents and out of scope for this change).
//! `src/main.rs` keeps its own local `mod config; mod models; mod modules;
//! mod services;` tree exactly as before - unrelated to this lib target -
//! so the binary crate is untouched. Keeping `modules` out of this lib
//! means the benches/tests introduced alongside this file do not need to
//! compile `modules::api` at all, and are therefore unaffected by it.
pub mod config;
pub mod models;
pub mod services;

/// Only the `env` submodule of `modules` - `config::performance` needs
/// `EnvConfig` for its `From<&EnvConfig>` impl. Resolves to the same
/// `src/modules/env/` files `main.rs`'s own local module tree uses; nothing
/// under `modules::api` is reachable through this path.
pub mod modules {
    pub mod env;

    /// Only the leaf helpers `config::performance` needs. `cgroup` has no
    /// dependencies of its own, so exposing it here does not drag the
    /// axum-dependent parts of `modules::utils` (`err`, `etag`) into the
    /// lib target.
    ///
    /// `crypto` is here for the same reason and on the same terms - it
    /// depends only on `rustls-graviola`. It has to be reachable from the
    /// lib target because `services::image` and `services::resize` build
    /// real `reqwest::Client`s, which panic unless a rustls crypto provider
    /// has been installed, and their tests compile as part of *this* crate,
    /// not the binary. Leaving it out of this list is a confusing failure:
    /// the module and its `mod` declaration both exist, and the error is
    /// still "cannot find `crypto` in `utils`", because this restricted
    /// re-declaration - not `modules/utils/mod.rs` - is what the lib sees.
    pub mod utils {
        pub mod cgroup;
        pub mod crypto;
    }

    /// Exposed so `src/bin/benchmark.rs` can sign the URLs it generates and
    /// therefore exercise the real verification path, rather than only ever
    /// hitting the `unsigned` escape.
    pub mod signing;
}
