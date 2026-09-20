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

/// Test-only equivalent of [`install_crypto_provider`], safe to call from any
/// number of tests in any order.
///
/// `install_default` fails if a provider is already installed, and Rust runs
/// tests concurrently in one process, so the first test to get there would
/// win and every other would error. `Once` makes the call idempotent, and the
/// result is deliberately ignored: by the time a second caller arrives the
/// provider is installed, which is all the caller actually needs.
#[cfg(test)]
pub fn ensure_crypto_provider_for_tests() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = install_crypto_provider();
    });
}
