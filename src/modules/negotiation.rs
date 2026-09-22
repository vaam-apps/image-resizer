//! `Accept`-driven content negotiation for the `.auto` output-format
//! extension (#49).
//!
//! `crate::modules::api::resize::handle` calls [`resolve`] once, right
//! after the URL is parsed and before a `ResizeQuery` is built - every
//! other layer (`CacheService::generate_key`, `ImageService`) only ever
//! sees a concrete, already-negotiated `ImageFormat`. This module was
//! reconstructed during wave-2 integration: the `#49` patch declared
//! `pub mod negotiation;` and called `negotiation::resolve(...)`, but the
//! module file itself was missing from the patch. The implementation below
//! matches every call site and test the patch shipped
//! (`src/modules/api/resize.rs`'s `auto_extension_negotiates_and_sets_vary_header`
//! / `explicit_format_request_has_no_vary_header` tests, and
//! `ImageFormat::Auto`'s own doc comment in `src/models/params.rs`).

use crate::models::params::ImageFormat;

/// Resolves `format` against the request's `Accept` header, if `format` is
/// [`ImageFormat::Auto`]. Returns the concrete format to actually produce,
/// plus whether negotiation happened at all - the caller uses the latter to
/// decide whether the response needs a `Vary: Accept` header, since only a
/// negotiated result actually depends on `Accept`.
///
/// Any format other than `Auto` passes through unchanged with
/// `negotiated = false` - an explicit `.jpg`/`.png`/`.webp`/`.avif`/`.gif`
/// request's output is fully determined by the URL, never by `Accept`.
///
/// When `format` is `Auto`, the concrete format is chosen from those the
/// client actually advertises support for, in order: [`ImageFormat::Avif`],
/// then [`ImageFormat::Webp`], falling back to [`ImageFormat::Jpg`] when
/// neither is accepted (including when `accept` is `None`, or fails to
/// parse as a comma-separated `Accept` list). Preference is weighted by
/// each entry's `q` parameter (default `1.0` when absent) - a client that
/// explicitly deprioritises AVIF below WebP (`image/avif;q=0.3,
/// image/webp;q=0.8`) gets WebP, not AVIF-by-listed-order.
///
/// # Why this order (#135)
///
/// The two steps are preferred for different reasons, and only the first
/// one is about size. Numbers are from `adr/0006-pure-rust-codecs.md`,
/// measured across 24 photographs at three matched-DSSIM targets, relative
/// to the same build's JPEG.
///
/// **AVIF first, on size.** 0.685x / 0.784x / 0.828x - 17-32% smaller than
/// JPEG, and ahead on SSIMULACRA2 as well as on the DSSIM target it was
/// matched against, so the win is not an artifact of the metric.
///
/// **WebP over JPEG, on alpha - not on size.** This used to be a size
/// preference and no longer is: WebP now measures 0.964x / 1.067x / 1.129x,
/// i.e. 3.6% smaller at the loosest target and *larger* above it, and it
/// lands ~2.2 SSIMULACRA2 points below AVIF at DSSIM-matched size. Treat
/// the two as equivalent on bytes. What WebP still has, and JPEG
/// structurally cannot, is an alpha channel: a JPEG output flattens onto a
/// background (`ImageService::flatten_onto_background`,
/// `src/services/image/handler.rs`). A client that accepts WebP but not
/// AVIF has no other alpha-capable option, which is the whole reason this
/// arm exists.
///
/// The cost that is *not* accounted for here is encode time - WebP encodes
/// in ~112-128 ms against JPEG's ~2.8-3.0 ms in this build. That is paid
/// once per image and amortised by the cache, so it does not belong in a
/// per-request negotiation decision; reducing it is #136.
pub fn resolve(format: ImageFormat, accept: Option<&str>) -> (ImageFormat, bool) {
    if format != ImageFormat::Auto {
        return (format, false);
    }

    let entries = accept.map(parse_accept).unwrap_or_default();

    let avif_q = best_q(&entries, "image/avif").unwrap_or(0.0);
    let webp_q = best_q(&entries, "image/webp").unwrap_or(0.0);

    // Ties (including both at the default `1.0`, e.g. a bare `image/*`
    // wildcard) go to AVIF - 17-32% smaller than JPEG where WebP is now
    // roughly at parity with it. See this function's own doc comment.
    let resolved = if avif_q > 0.0 && avif_q >= webp_q {
        ImageFormat::Avif
    } else if webp_q > 0.0 {
        ImageFormat::Webp
    } else {
        ImageFormat::Jpg
    };

    (resolved, true)
}

/// One parsed `Accept` media-range entry: its media type (lowercased,
/// whitespace-trimmed) and `q` value.
struct AcceptEntry {
    media_type: String,
    q: f32,
}

/// Splits an `Accept` header value into its comma-separated media ranges,
/// parsing each range's optional `;q=` parameter. Malformed entries
/// (unparseable `q`) fall back to `q = 1.0` rather than being dropped -
/// being lenient here just means a slightly-wrong preference weight, never
/// a hard failure for a header this crate doesn't otherwise validate.
fn parse_accept(accept: &str) -> Vec<AcceptEntry> {
    accept
        .split(',')
        .filter_map(|entry| {
            let mut parts = entry.split(';');
            let media_type = parts.next()?.trim().to_ascii_lowercase();
            if media_type.is_empty() {
                return None;
            }

            let q = parts
                .filter_map(|param| {
                    let param = param.trim();
                    param.strip_prefix("q=").and_then(|v| v.trim().parse().ok())
                })
                .next()
                .unwrap_or(1.0);

            Some(AcceptEntry { media_type, q })
        })
        .collect()
}

/// The highest `q` among `entries` that accepts `media_type` - either via
/// an exact match, the `image/*` wildcard, or the `*/*` wildcard. `None`
/// means `media_type` isn't accepted at all (no matching entry, wildcard or
/// otherwise).
fn best_q(entries: &[AcceptEntry], media_type: &str) -> Option<f32> {
    entries
        .iter()
        .filter(|entry| entry.media_type == media_type || entry.media_type == "image/*" || entry.media_type == "*/*")
        .map(|entry| entry.q)
        .fold(None, |acc, q| Some(acc.map_or(q, |a: f32| a.max(q))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-`Auto` format always passes through unchanged, and never
    /// counts as negotiated - `resize::handle` relies on this to decide
    /// whether to set `Vary: Accept`.
    #[test]
    fn non_auto_format_passes_through_unnegotiated() {
        for format in [
            ImageFormat::Jpg,
            ImageFormat::Png,
            ImageFormat::Webp,
            ImageFormat::Avif,
            ImageFormat::Gif,
        ] {
            assert_eq!(resolve(format, Some("image/avif")), (format, false));
            assert_eq!(resolve(format, None), (format, false));
        }
    }

    /// An `Accept` header naming AVIF (with no explicit `q`, so default
    /// `1.0`, at least as high as any other candidate) resolves to AVIF -
    /// mirrors `resize::auto_extension_negotiates_and_sets_vary_header`.
    #[test]
    fn auto_prefers_avif_when_accepted() {
        assert_eq!(
            resolve(
                ImageFormat::Auto,
                Some("image/avif,image/webp,image/*;q=0.8")
            ),
            (ImageFormat::Avif, true)
        );
    }

    /// AVIF absent, WebP present -> WebP, still ahead of the JPEG fallback.
    ///
    /// Deliberate, and load-bearing (#135): WebP is no longer smaller than
    /// JPEG - it measures 0.964x / 1.067x / 1.129x against it - so a reader
    /// who only checks the size table will conclude this arm is obsolete
    /// and flip it. It is kept for **alpha**. JPEG has no alpha channel and
    /// is flattened onto a background, and a client on this branch (accepts
    /// WebP, not AVIF) has no other alpha-capable option. Do not reorder
    /// this without an answer for transparent sources.
    #[test]
    fn auto_prefers_webp_when_avif_not_accepted() {
        assert_eq!(
            resolve(ImageFormat::Auto, Some("image/webp,image/*;q=0.8")),
            (ImageFormat::Webp, true)
        );
    }

    /// Neither AVIF nor WebP accepted -> falls back to JPEG, but is still
    /// reported as negotiated (`Auto` was resolved, even if the outcome is
    /// the same as the pre-#49 default).
    #[test]
    fn auto_falls_back_to_jpeg_when_neither_accepted() {
        assert_eq!(
            resolve(ImageFormat::Auto, Some("text/html,image/png")),
            (ImageFormat::Jpg, true)
        );
    }

    /// No `Accept` header at all -> same JPEG fallback as an
    /// explicitly-unsupportive header, not a hard error.
    #[test]
    fn auto_falls_back_to_jpeg_when_accept_header_missing() {
        assert_eq!(resolve(ImageFormat::Auto, None), (ImageFormat::Jpg, true));
    }

    /// A `q` weight lower than WebP's demotes AVIF below it, even though
    /// AVIF is listed first - preference follows the client's stated
    /// weighting, not header order.
    #[test]
    fn auto_respects_explicit_q_weighting_over_listed_order() {
        assert_eq!(
            resolve(
                ImageFormat::Auto,
                Some("image/avif;q=0.3, image/webp;q=0.8")
            ),
            (ImageFormat::Webp, true)
        );
    }

    /// `q=0` is an explicit rejection, not "unspecified, default to 1.0" -
    /// an AVIF entry with `q=0` must not be selected even though it's
    /// present in the header.
    #[test]
    fn auto_treats_q_zero_as_rejected() {
        assert_eq!(
            resolve(ImageFormat::Auto, Some("image/avif;q=0, image/webp")),
            (ImageFormat::Webp, true)
        );
    }

    /// A bare `image/*` wildcard (no explicit AVIF/WebP entry) accepts
    /// both at the wildcard's own `q`, and AVIF wins the resulting tie by
    /// preference order.
    #[test]
    fn auto_resolves_image_wildcard_to_avif_preference() {
        assert_eq!(
            resolve(ImageFormat::Auto, Some("image/*")),
            (ImageFormat::Avif, true)
        );
    }
}
