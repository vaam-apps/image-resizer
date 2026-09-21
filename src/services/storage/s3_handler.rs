use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_sdk_s3 as s3;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::primitives::{ByteStream, DateTime, DateTimeFormat};

use crate::services::storage::core::StorageBackend;

/// Builds the HTTP/TLS connector `MinIOStorage::new_minio` hands to the S3
/// SDK (#134).
///
/// `aws-sdk-s3`'s own "default-https-client" Cargo feature has been dropped
/// (see the dependency comment in Cargo.toml) precisely because its only
/// choices - `aws-smithy-http-client`'s `rustls-ring`, `rustls-aws-lc` and
/// `rustls-aws-lc-fips` features - are all C-and-assembly crypto backends,
/// the exact thing this crate is removing everywhere else. With that
/// feature gone, aws-sdk-s3 builds no HTTP client implicitly at all, so
/// skipping this function is not an option: `new_minio` would fail at
/// runtime with no way to make a request, not just lose TLS.
///
/// `aws_smithy_http_client::tls::rustls_provider::CryptoMode::Custom` is
/// smithy-rs's escape hatch for supplying a caller-built `CryptoProvider`
/// instead of picking one of its built-in modes; it only exists at all
/// behind the `aws_sdk_unstable` `--cfg` flag set in `.cargo/config.toml`
/// (upstream's signal that the shape of this API may change between
/// releases - accepted here because it is the only way to route the S3 SDK
/// through a pure-Rust crypto provider).
///
/// Reads back `rustls::crypto::CryptoProvider::get_default()` - the exact
/// Graviola instance `main::install_crypto_provider()` installs process-wide
/// at startup - rather than calling `rustls_graviola::default_provider()`
/// again here. Both would be behaviourally identical (it is a pure
/// constructor over `'static` data), but reusing the installed default
/// keeps this connector provably "whatever provider the process is running
/// on" instead of a second, independently-constructed instance that happens
/// to currently match.
fn build_https_client() -> s3::config::SharedHttpClient {
    let provider = rustls::crypto::CryptoProvider::get_default()
        .expect(
            "rustls default crypto provider must already be installed by \
             main::install_crypto_provider() before any S3 client is built",
        )
        .as_ref()
        .clone();

    aws_smithy_http_client::Builder::new()
        .tls_provider(aws_smithy_http_client::tls::Provider::rustls(
            aws_smithy_http_client::tls::rustls_provider::CryptoMode::Custom(provider),
        ))
        .build_https()
}

/// MinIO storage implementation
pub struct MinIOStorage {
    client: s3::Client,
    bucket: String,
}

impl MinIOStorage {
    pub fn new_minio(
        endpoint_url: String,
        access_key: String,
        secret_key: String,
        bucket: String,
        region: String,
    ) -> anyhow::Result<Self> {
        let s3_config = s3::config::Builder::new()
            .endpoint_url(endpoint_url)
            .credentials_provider(s3::config::Credentials::new(
                access_key, secret_key, None,     // session_token
                None,     // expiry
                "Static", // provider_name
            ))
            .region(s3::config::Region::new(region))
            .force_path_style(true) // Crucial for MinIO compatibility
            .http_client(build_https_client()) // (#134) see that fn's doc comment
            .build();

        let s3_client = s3::Client::from_conf(s3_config);

        Ok(Self {
            client: s3_client,
            bucket,
        })
    }

    /// Whether an S3 `Expires` header value (as returned by `head_object`/
    /// `get_object`) means the object should be treated as absent (#40).
    /// `None` (no `Expires` set on the object, i.e. `upload_image_with_ttl`
    /// was called with `ttl: None`) means "never expires". Takes the raw
    /// header string (`expires_string()`) rather than the deprecated typed
    /// `expires()` accessor, and fails open (treats unparsable as "not
    /// expired") on a malformed value - same fail-open contract as every
    /// other backend's expiry check.
    fn is_expired(expires: Option<&str>) -> bool {
        let Some(expires) = expires else {
            return false;
        };
        let Ok(expires) = DateTime::from_str(expires, DateTimeFormat::HttpDate) else {
            return false;
        };
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        now_secs >= expires.secs()
    }
}

#[async_trait]
impl StorageBackend for MinIOStorage {
    async fn upload_image_with_ttl(
        &self,
        key: &str,
        content_type: &str,
        data: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .content_type(content_type)
            // Cache-Control was previously left unset on the object itself;
            // this now matches the header the download response is served
            // with (`src/modules/api/resize.rs`), so it's also correct for
            // anything reading the object directly from S3/MinIO/a CDN
            // fronting the bucket, not just via this app's own endpoint.
            .cache_control("public, max-age=31536000, immutable");

        // TTL concept (#40): set the S3 object's own `Expires` metadata so
        // `check_cache`/`get_image` can lazily evict it on the next read,
        // without a separate background sweep or lifecycle rule to manage.
        if let Some(ttl) = ttl {
            if let Some(expires_at) = SystemTime::now().checked_add(ttl) {
                request = request.expires(DateTime::from(expires_at));
            }
        }

        request
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 error: {}", e))
            .context("Failed to upload image to MinIO")?;
        Ok(())
    }

    async fn check_cache(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => {
                if Self::is_expired(output.expires_string()) {
                    // Lazy eviction (#40): best-effort - if the delete
                    // fails or races another reader, the entry is still
                    // correctly reported as a miss here; the object is just
                    // cleaned up a little later.
                    let _ = self.delete(key).await;
                    return Ok(false);
                }
                Ok(true)
            }
            Err(sdk_err) => match sdk_err.into_service_error() {
                HeadObjectError::NotFound(_) => Ok(false),
                err => Err(anyhow::anyhow!("S3 error: {}", err)),
            },
        }
    }

    async fn get_image(&self, key: &str) -> Result<Vec<u8>> {
        let response = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(sdk_err) => {
                return match sdk_err.into_service_error() {
                    // Matched by message content, not type, one layer up
                    // (`AppError::classify_download_error`, `src/modules/utils/err.rs`,
                    // owned separately) - "not found" must appear in the text.
                    GetObjectError::NoSuchKey(_) => {
                        Err(anyhow::anyhow!("Image not found in S3: {}", key))
                    }
                    err => Err(anyhow::anyhow!("S3 error: {}", err))
                        .context(format!("Failed to get image from S3: {}", key)),
                };
            }
        };

        if Self::is_expired(response.expires_string()) {
            // Same lazy-eviction contract as `check_cache` (#40): an
            // expired entry must fail exactly like a missing one.
            let _ = self.delete(key).await;
            return Err(anyhow::anyhow!("Image not found in S3: {} (expired)", key));
        }

        let data = response
            .body
            .collect()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read S3 response body: {}", e))?;

        Ok(data.into_bytes().to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        // S3's DeleteObject is itself idempotent - it returns success
        // whether or not the key exists - so no NotFound special-casing is
        // needed here (#40).
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 error: {}", e))
            .context(format!("Failed to delete image from S3: {}", key))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test` never runs `main()`, so `main::install_crypto_provider()`
    /// never executes for this test binary (#134) - but `new_minio` now
    /// calls `build_https_client()`, which panics via `.expect()` if no
    /// rustls default crypto provider is installed. `s3_handler.rs` is
    /// compiled into both the `emgr` bin (via `main.rs`'s local module
    /// tree) and the `emgr` lib (via `lib.rs`'s `pub mod services`) as two
    /// separate crate instances of the same source file, so this test
    /// cannot reach `main.rs`'s own test-only installer - it needs its own.
    /// `Once`, not a bare call, for the same reason as `main.rs`'s
    /// equivalent: `install_default` succeeds at most once per process, and
    /// `cargo test` runs tests in parallel by default.
    fn ensure_crypto_provider_for_tests() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = rustls_graviola::default_provider().install_default();
        });
    }

    /// `new_minio` only builds an `aws_sdk_s3::Client` from a static config
    /// (`s3::config::Builder`) - the AWS SDK is lazy, so this never makes a
    /// network call. Confirms construction succeeds with typical inputs
    /// (this also doubles as the "does this compile/link with force_path_style"
    /// smoke test).
    #[test]
    fn new_minio_succeeds_with_typical_inputs_and_makes_no_network_call() {
        ensure_crypto_provider_for_tests();
        let result = MinIOStorage::new_minio(
            "http://127.0.0.1:9999".to_string(),
            "test-access-key".to_string(),
            "test-secret-key".to_string(),
            "test-bucket".to_string(),
            "us-east-1".to_string(),
        );
        assert!(
            result.is_ok(),
            "new_minio should succeed with no network involved"
        );
    }

    /// `None` (no `Expires` set - i.e. `upload_image_with_ttl` was called
    /// with `ttl: None`) must mean "never expires".
    #[test]
    fn is_expired_none_is_never_expired() {
        assert!(!MinIOStorage::is_expired(None));
    }

    /// A well-formed HTTP-date clearly in the future must not be expired.
    #[test]
    fn is_expired_future_http_date_is_not_expired() {
        let future = SystemTime::now() + Duration::from_secs(3600);
        let http_date = DateTime::from(future)
            .fmt(DateTimeFormat::HttpDate)
            .expect("format future date as HTTP-date");
        assert!(!MinIOStorage::is_expired(Some(&http_date)));
    }

    /// A well-formed HTTP-date clearly in the past must be expired.
    #[test]
    fn is_expired_past_http_date_is_expired() {
        let past = SystemTime::now() - Duration::from_secs(3600);
        let http_date = DateTime::from(past)
            .fmt(DateTimeFormat::HttpDate)
            .expect("format past date as HTTP-date");
        assert!(MinIOStorage::is_expired(Some(&http_date)));
    }

    /// An unparsable/garbage `Expires` value must fail open (treated as
    /// "not expired") - same contract as every other backend's expiry
    /// check, and safer than treating a malformed value as an outright
    /// eviction trigger.
    #[test]
    fn is_expired_malformed_string_fails_open() {
        assert!(!MinIOStorage::is_expired(Some("not-a-real-http-date")));
        assert!(!MinIOStorage::is_expired(Some("")));
        assert!(!MinIOStorage::is_expired(Some(
            "Wed, 99 Foo 9999 99:99:99 GMT"
        )));
    }

    /// Right at the boundary: `is_expired` uses `now_secs >= expires.secs()`,
    /// so a value equal to "now" (truncated to whole seconds, matching
    /// HTTP-date's own second-granularity) must already count as expired.
    #[test]
    fn is_expired_boundary_at_exactly_now_is_expired() {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_secs() as i64;
        // HttpDate only carries second-granularity, so DateTime::from_secs
        // round-trips exactly through fmt/parse - no truncation surprises.
        let now = DateTime::from_secs(now_secs);
        let http_date = now
            .fmt(DateTimeFormat::HttpDate)
            .expect("format boundary date as HTTP-date");
        assert!(MinIOStorage::is_expired(Some(&http_date)));
    }
}
