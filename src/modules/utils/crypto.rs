//! Process-wide rustls crypto provider installation.
//!
//! `reqwest` is built with `rustls-no-provider` so that `aws-lc-rs` - a large
//! C and assembly codebase - stays out of the dependency graph. The trade-off
//! is that rustls then has no crypto provider compiled in, and **reqwest
//! panics when a `Client` is built** unless a process-wide default has been
//! installed first. Every TLS-capable path in this service therefore depends
//! on [`install_crypto_provider`] having already run.
//!
//! This lives in the library rather than in `main.rs` because `cargo test`
//! never runs `main()`, and several test modules build real `reqwest::Client`s
//! through production code paths (`ImageService::build_pinned_client`, the S3
//! handler). Keeping one implementation here avoids the alternative that was
//! tried first: a hand-rolled copy of it in each test module that needed one,
//! which is how three of them ended up slightly different.

/// Installs Graviola as the process-wide rustls crypto provider.
///
/// Call this once, as early in `main` as possible, and before anything can
/// construct a `reqwest::Client`. Failing loudly here is deliberate: a service
/// that cannot do TLS should not come up reporting itself healthy.
pub fn install_crypto_provider() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rustls_graviola::default_provider()
        .install_default()
        .map_err(|_already_installed| -> Box<dyn std::error::Error + Send + Sync> {
            // The `Err` payload is the `Arc<CryptoProvider>` that lost the
            // race. It is a plain data struct - cipher suites, kx groups - and
            // does not implement `std::error::Error`, so it cannot be
            // forwarded as-is. Reaching this branch means something outside
            // this codebase installed a provider first, which is exactly the
            // "do not start with an unknown TLS backend" case.
            "a rustls crypto provider was already installed before Graviola \
             could be - refusing to start with an unknown TLS backend"
                .into()
        })
}

/// Idempotent equivalent of [`install_crypto_provider`], safe to call any number
/// of times from any number of threads.
///
/// Deliberately **not** `#[cfg(test)]`. Integration tests under `tests/` link
/// this crate compiled *without* `cfg(test)`, so a test-gated helper is
/// invisible to them - `tests/storage_s3_handler.rs` builds a real S3 client,
/// which needs a provider, and would panic in `build_https_client`.
///
/// `install_default` fails if a provider is already installed, and Rust runs
/// tests concurrently in one process, so the first test to get there would
/// win and every other would error. `Once` makes the call idempotent, and the
/// result is deliberately ignored: by the time a second caller arrives the
/// provider is installed, which is all the caller actually needs.
/// A `reqwest::Client` for tests, with the crypto provider guaranteed installed.
///
/// Prefer this over `reqwest::Client::new()` in test code. Not a style
/// preference: `Client::new()` **panics** when no provider has been installed,
/// and whether one has depends on whether some *other* test in the same binary
/// happened to install one first. That makes a bare `Client::new()` pass or
/// fail depending on the feature set and the test-ordering - which is exactly
/// how `non_download_routes_are_left_untouched` passed under `local_fs` and
/// failed under `s3`, having been missed when the other tests in its own file
/// were fixed.
///
/// Routing construction through here makes the dependency structural: you
/// cannot obtain a Client without the installer having run.
#[cfg(test)]
pub fn test_http_client() -> reqwest::Client {
    ensure_crypto_provider_installed();
    reqwest::Client::new()
}

/// Former name of [`ensure_crypto_provider_installed`], kept for in-crate test
/// call sites.
///
/// The public function had to lose its `#[cfg(test)]` gate so integration tests
/// under `tests/` could reach it; this alias means that change did not have to
/// touch every unit test at the same time.
#[cfg(test)]
pub fn ensure_crypto_provider_for_tests() {
    ensure_crypto_provider_installed();
}

pub fn ensure_crypto_provider_installed() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = install_crypto_provider();
    });
}
