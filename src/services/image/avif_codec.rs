//! AVIF encode/decode via pure-Rust codecs (C-dependency removal, #67/#68's
//! `libavif-sys` FFI path replaced) - `ravif` for encode, `avif-decode` for
//! decode. See `Cargo.toml`'s own comment on the `ravif`/`avif-decode`
//! dependency block for why these two crates specifically (one per
//! direction, rather than a single AVIF crate covering both - no such
//! all-pure-Rust crate exists).
//!
//! # `avif-parse` must become a *direct* dependency - this file will not
//! compile without that `Cargo.toml` change
//!
//! `avif-decode`'s `Decoder` (its only public entry point) has no API that
//! returns dimensions, or anything else, without first running the actual
//! AV1 frame decode through `rav1d` (`avif-decode-3.0.0/src/lib.rs`'s
//! `Decoder::to_image` is the only way to reach a width/height, and it
//! unconditionally calls `Rav1dDecoder::decode_frame` first). That is
//! exactly the expensive step `peek_dimensions` and `decode`'s pre-decode
//! resolution guard both exist to avoid running on an oversized or
//! adversarial input (see those functions' own doc comments).
//!
//! The only place dimensions are available *before* an AV1 frame decode is
//! `avif_parse::AvifData::primary_item_metadata()` (parses just the AV1
//! sequence header OBU, not a full frame decode) - `avif-parse` 2.1.0 is
//! already in this workspace's `Cargo.lock` (pinned exactly by
//! `avif-decode`'s own `Cargo.toml`, `avif-parse = "2.1.0"`), but only as a
//! *transitive* dependency. Rust's 2018+ extern prelude only auto-resolves
//! crates a package lists directly in its own `Cargo.toml`
//! `[dependencies]` - so `use avif_parse::...` below fails to resolve
//! (`error[E0433]`) until `avif-parse = "2"` is added there too. That
//! addition cannot move the resolved version (already pinned at `2.1.0` by
//! `avif-decode`), so it carries no version-resolution risk - it only
//! promotes an already-locked transitive dependency to direct. This
//! module's own scope is one file; the `Cargo.toml` edit is reported
//! alongside this change rather than made here.
use anyhow::{Context, Result};
use image::DynamicImage;
use image::metadata::Orientation;

/// AVIF-specific slice of the decode tuple every other decode path in
/// `handler.rs` returns as `DecodedImage` (`(DynamicImage, Orientation,
/// Option<Vec<u8>>, Option<Vec<u8>>)`), kept as an inline tuple here rather
/// than importing that private type alias across the module boundary.
pub type AvifDecoded = (DynamicImage, Orientation, Option<Vec<u8>>, Option<Vec<u8>>);

/// Detects an AVIF source via `avif_parse::read_avif` succeeding - the
/// closest available "ground truth" now that libavif's own
/// `avifPeekCompatibleFileType` (what this test used pre-#67/#68) is gone
/// with `libavif-sys`. Unlike that old peek (a pure `ftyp`-brand check, no
/// deeper parse), `read_avif` fully parses the container - meta box, item
/// properties, item locations - so this is now a stricter "is this a
/// complete, well-formed still AVIF" check rather than a magic-byte sniff.
/// That's still a meaningful independent cross-check for
/// `handler.rs::ImageService::is_avif`'s hand-rolled magic-byte check (kept
/// FFI/parse-free there so `detect_format_from_bytes` doesn't need to link
/// an AVIF parser just to sniff a format tag - see that function's own doc
/// comment): the two now disagree on truncated/corrupt-but-`ftyp`-tagged
/// input where they used to agree, which is a known, accepted narrowing of
/// what this test proves, not a bug.
#[cfg(test)]
pub(crate) fn is_avif(bytes: &[u8]) -> bool {
    avif_parse::read_avif(&mut &bytes[..]).is_ok()
}

/// Reads only the AVIF container header and AV1 sequence-header OBU
/// (`avif_parse::read_avif` + `AvifData::primary_item_metadata`) to get the
/// image's dimensions, without running the AV1 frame decode - AVIF's
/// equivalent of `ImageReader::into_dimensions()`, which can't parse AVIF
/// at all without the `avif-native` feature this crate doesn't enable (see
/// `Cargo.toml`'s `image` dependency comment). Called from
/// `ImageService::peek_dimensions` *before* `check_source_resolution` and
/// any pixel decode, exactly like every other format's header peek.
///
/// **Not as cheap as the old libavif-backed peek.** `avif_parse::read_avif`
/// copies the primary item's *coded* AV1 payload bytes into memory as part
/// of parsing (there is no API to stop at box-structure-only) - the old
/// `avifDecoderParse` call this replaces did not need to. That copy is
/// still bounded by the attacker-controlled input's own byte length (no
/// amplification: the caller already sent every one of those bytes over
/// the wire) and, critically, never invokes the AV1 decoder itself
/// (`rav1d`) - the expensive, potentially-bomb-amplifying step stays
/// exactly where it was, in `decode`, after the resolution check. So this
/// is a real but bounded cost increase (a memcpy of up to the request's own
/// byte size), not the "full decode" DoS regression this function's own
/// contract warns against - see this module's own doc comment for why
/// `avif_parse` (rather than `avif-decode`'s own `Decoder`) is what makes
/// this possible at all.
pub fn peek_dimensions(image_bytes: &[u8]) -> Result<(u32, u32)> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        peek_dimensions_inner(image_bytes)
    }))
    .unwrap_or_else(|payload| Err(panic_to_error(payload, "avif-parse header parse")))
}

fn peek_dimensions_inner(image_bytes: &[u8]) -> Result<(u32, u32)> {
    let avif = avif_parse::read_avif(&mut &image_bytes[..])
        .context("avif-parse: failed to parse AVIF container")?;
    let meta = avif
        .primary_item_metadata()
        .context("avif-parse: failed to parse AV1 sequence header")?;
    Ok((meta.max_frame_width.get(), meta.max_frame_height.get()))
}

/// Decodes an AVIF source, enforcing the same resolution guard every other
/// format's decode path does (#67):
/// - The container and AV1 sequence header are parsed first via
///   `avif_parse::read_avif`/`primary_item_metadata` (the same cheap,
///   frame-decode-free path `peek_dimensions` uses - see that function's
///   own doc comment on what "cheap" means here).
/// - `check_source_resolution` runs against those header-parsed dimensions
///   *before* `avif_decode::Decoder::to_image` - the call that actually
///   runs the AV1 frame decode through `rav1d` - is ever reached. This is
///   what makes an AVIF decompression bomb fail the same way a
///   JPEG/PNG/WebP one does: rejected from a cheap header read, never
///   reaching the expensive pixel decode. The *primary* guard
///   (`ImageService::peek_dimensions` -> `check_source_resolution`, run by
///   the caller before `decode_with_limits`/this function is ever reached)
///   already covers this - the check here is defense in depth, matching
///   every other decode path's "primary check upstream, guard repeated at
///   the point of actual decode" structure.
///
/// The container ends up parsed twice - once here via `avif_parse`
/// directly for the guard, once more inside `avif_decode::Decoder::from_avif`
/// for the actual decode - because `avif_decode::Decoder`'s fields are
/// private with no constructor from an already-parsed `AvifData` (see this
/// module's own doc comment). Both parses are the same cheap,
/// frame-decode-free step; only one AV1 frame decode ever runs, and only
/// after the guard passes, so the duplication doesn't weaken it.
///
/// No equivalent of the old `imageCountLimit = 1` (animated-AVIF guard) is
/// needed: `avif_parse::read_avif` rejects the animated-AVIF `ftyp` brand
/// (`"avis"`) outright with `Error::Unsupported` before returning anything
/// - still images are the only thing that can ever reach this function's
/// decode step, structurally, not by a limit this module sets.
///
/// # Panics (caught, not propagated)
///
/// The whole sequence (container parse through pixel conversion) runs
/// inside one `catch_unwind`, same defensive spirit as
/// `mozjpeg_decode`/`libwebp_decode` in `handler.rs`: neither `avif-parse`
/// nor `avif-decode`/`rav1d` is expected to panic on malformed input, but
/// `Cargo.toml`'s `panic = "unwind"` (kept for #29, decoding untrusted
/// input) is what makes any unexpected panic here catchable rather than
/// fatal to the whole worker thread, and this function is reached with
/// attacker-supplied bytes on every AVIF-source request.
pub fn decode(image_bytes: &[u8], max_src_resolution_mp: u64) -> Result<AvifDecoded> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decode_inner(image_bytes, max_src_resolution_mp)
    }))
    .unwrap_or_else(|payload| Err(panic_to_error(payload, "avif-decode decode")))
}

fn decode_inner(image_bytes: &[u8], max_src_resolution_mp: u64) -> Result<AvifDecoded> {
    let avif = avif_parse::read_avif(&mut &image_bytes[..])
        .context("avif-parse: failed to parse AVIF container")?;
    let meta = avif
        .primary_item_metadata()
        .context("avif-parse: failed to parse AV1 sequence header")?;
    let (width, height) = (meta.max_frame_width.get(), meta.max_frame_height.get());

    // Defense in depth - see this function's own doc comment; the primary
    // check already ran in `ImageService::peek_dimensions` against these
    // same header-parsed dimensions.
    crate::services::image::handler::ImageService::check_source_resolution(
        width,
        height,
        max_src_resolution_mp,
    )?;

    // The expensive step: runs the AV1 frame decode through `rav1d`, only
    // now that the resolution guard above has passed. `avif-decode` already
    // performs YUV->RGB conversion (chroma upsampling for 4:2:0/4:2:2, or a
    // pass-through for 4:4:4/monochrome) and bit-depth normalisation to a
    // full 8-bit or full 16-bit range internally (see
    // `avif-decode-3.0.0/src/image.rs`'s `yuv_to_rgb8`/`yuv_to_rgb16`/
    // `luma16`) - unlike the old libavif path, which had to call
    // `avifImageYUVToRGB` itself. `avif_image_to_dynamic` below only needs
    // to reshape the already-converted pixels into a `DynamicImage`.
    let decoded = avif_decode::Decoder::from_avif(image_bytes)
        .context("avif-decode: failed to parse AVIF container")?
        .to_image()
        .context("avif-decode: failed to decode AV1 frame")?;

    let dynamic_image = avif_image_to_dynamic(decoded)?;

    // Orientation (`irot`/`imir`) and both metadata kinds (ICC `colr`, EXIF
    // `Exif` item) are unrecoverable on this path - not a choice made in
    // this module, a hard capability gap in `avif-parse` 2.1.0 itself.
    // Its `AvifData` (`avif-parse-2.1.0/src/lib.rs`) carries only
    // `primary_item`, `alpha_item`, `premultiplied_alpha`,
    // `content_light_level` and `mastering_display` - no ICC/EXIF/transform
    // fields exist to populate. Its internal `ItemProperty` enum
    // (`avif-parse-2.1.0/src/lib.rs:1091`) has variants for exactly
    // `Channels`/`AuxiliaryType`/`ContentLightLevel`/
    // `MasteringDisplayColourVolume`, plus a catch-all `Unsupported` that
    // silently swallows every other item property box - including `colr`
    // (ICC), `irot`, and `imir`. The `Exif`-typed item itself (a separate
    // `infe`/`iloc` item referenced via an `'cdsc'` item reference, not a
    // property at all) is never looked at either. This is a real,
    // reportable behaviour change from the old libavif path (which read
    // `image->icc`/`image->exif` and inverted `image->transformFlags`/
    // `irot`/`imir` via `avif_orientation`, both removed with this change):
    // a decoded AVIF's orientation is always treated as already-correct
    // (`Orientation::NoTransforms`, the same "malformed/absent metadata
    // isn't worth failing the whole request over" default every other
    // format in `handler.rs` falls back to), and its ICC/EXIF are always
    // `None`, even when the source file actually embeds them. Note this is
    // asymmetric with `encode` (below), whose `ravif::Encoder::with_exif`
    // *does* write a real EXIF item - AVIFs this service produces itself
    // and later re-decodes will still lose that EXIF on the way back in.
    Ok((dynamic_image, Orientation::NoTransforms, None, None))
}

/// Converts `avif-decode`'s already YUV->RGB-converted, bit-depth-normalised
/// `Image` into this crate's `DynamicImage` - see `decode_inner`'s own
/// comment on what `avif-decode` already handled before this point.
///
/// Unlike the old libavif path, which always converted through
/// `AVIF_RGB_FORMAT_RGBA` and so always returned
/// `DynamicImage::ImageRgba8` regardless of the source, this preserves the
/// source's actual channel count and bit depth: an AVIF with no alpha item
/// decodes to `ImageRgb8`/`ImageRgb16`, a monochrome one to
/// `ImageLuma8`/`ImageLuma16`. A real, reportable change in what variant
/// callers get back - not obviously wrong (it stops fabricating an alpha
/// channel or widening 8-bit sources that don't need it), but different
/// from before, so calling out explicitly.
///
/// Each arm manually copies each pixel's components into a flat buffer
/// (rather than a zero-copy reinterpret cast) because `rgb`'s own
/// byte-casting trait (`ComponentBytes`) is deprecated upstream in favour
/// of `bytemuck::cast_slice`, and `bytemuck` isn't a dependency here - a
/// plain, safe per-pixel copy avoids both, matching this module's existing
/// preference for explicit, auditable copies over unsafe reinterpret casts
/// (the same choice the old `decode_inner`'s manual row-copy loop made).
fn avif_image_to_dynamic(image: avif_decode::Image) -> Result<DynamicImage> {
    fn dims(width: usize, height: usize) -> Result<(u32, u32)> {
        Ok((
            u32::try_from(width).context("avif-decode: image width does not fit in u32")?,
            u32::try_from(height).context("avif-decode: image height does not fit in u32")?,
        ))
    }

    match image {
        avif_decode::Image::Rgba8(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let mut bytes = Vec::with_capacity(buf.len() * 4);
            for px in buf {
                bytes.extend_from_slice(&[px.r, px.g, px.b, px.a]);
            }
            let img = image::RgbaImage::from_raw(width, height, bytes)
                .context("avif-decode: decoded RGBA8 buffer size mismatch")?;
            Ok(DynamicImage::ImageRgba8(img))
        }
        avif_decode::Image::Rgb8(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let mut bytes = Vec::with_capacity(buf.len() * 3);
            for px in buf {
                bytes.extend_from_slice(&[px.r, px.g, px.b]);
            }
            let img = image::RgbImage::from_raw(width, height, bytes)
                .context("avif-decode: decoded RGB8 buffer size mismatch")?;
            Ok(DynamicImage::ImageRgb8(img))
        }
        avif_decode::Image::Gray8(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let bytes: Vec<u8> = buf.into_iter().map(rgb::Gray::value).collect();
            let img = image::GrayImage::from_raw(width, height, bytes)
                .context("avif-decode: decoded Gray8 buffer size mismatch")?;
            Ok(DynamicImage::ImageLuma8(img))
        }
        avif_decode::Image::Rgba16(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let mut words = Vec::with_capacity(buf.len() * 4);
            for px in buf {
                words.extend_from_slice(&[px.r, px.g, px.b, px.a]);
            }
            let img: image::ImageBuffer<image::Rgba<u16>, Vec<u16>> =
                image::ImageBuffer::from_raw(width, height, words)
                    .context("avif-decode: decoded RGBA16 buffer size mismatch")?;
            Ok(DynamicImage::ImageRgba16(img))
        }
        avif_decode::Image::Rgb16(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let mut words = Vec::with_capacity(buf.len() * 3);
            for px in buf {
                words.extend_from_slice(&[px.r, px.g, px.b]);
            }
            let img: image::ImageBuffer<image::Rgb<u16>, Vec<u16>> =
                image::ImageBuffer::from_raw(width, height, words)
                    .context("avif-decode: decoded RGB16 buffer size mismatch")?;
            Ok(DynamicImage::ImageRgb16(img))
        }
        avif_decode::Image::Gray16(pixels) => {
            let (buf, width, height) = pixels.into_contiguous_buf();
            let (width, height) = dims(width, height)?;
            let words: Vec<u16> = buf.into_iter().map(rgb::Gray::value).collect();
            let img: image::ImageBuffer<image::Luma<u16>, Vec<u16>> =
                image::ImageBuffer::from_raw(width, height, words)
                    .context("avif-decode: decoded Gray16 buffer size mismatch")?;
            Ok(DynamicImage::ImageLuma16(img))
        }
    }
}

fn panic_to_error(payload: Box<dyn std::any::Any + Send>, what: &str) -> anyhow::Error {
    let msg = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| format!("{what} panicked with a non-string payload"));
    anyhow::anyhow!("{what} panicked: {msg}")
}

/// Encodes `img` to AVIF via `ravif`/`rav1e` (#68), replacing the
/// `libavif`+AOM encoder this crate used between #68 and this change - see
/// this module's own doc comment and `Cargo.toml`'s dependency comment for
/// why. `ravif`/`rav1e` is not a new choice for this crate: it's the same
/// encoder `adr/0004-avif-measurement.md` originally measured, before #67/
/// #68 swapped to libavif/AOM; this change reverts that swap on the encode
/// side specifically to shed the C toolchain, not because AOM's output was
/// found lacking.
///
/// # `quality`/`speed`: same public 0-100/0-10 numeric ranges, **not the
/// same scale** - translation is a pass-through, not a conversion
///
/// This function's public signature is unchanged: `quality` and `speed`
/// stay on the 0-100 / 0-10 scales `handler.rs`'s `DEFAULT_AVIF_QUALITY`/
/// `DEFAULT_AVIF_SPEED` and the `q:`/request-option surface already use.
/// Internally:
/// - `quality` (`u8`, 0-100) is passed as `f32` straight into
///   `ravif::Encoder::with_quality`, clamped to `1.0..=100.0` (`with_quality`
///   panics outside that range - `1..=100`, not `0..=100`, so an input of
///   `0` is floored to `1`, not translated).
/// - `speed` (`u8`, 0-10) is passed straight into `Encoder::with_speed`,
///   clamped to `1..=10` (same reason - `with_speed` panics on `0`).
///
/// **No numeric conversion is applied beyond that floor-clamp**, and that
/// is a deliberate choice, not an oversight: `adr/0005-avif-measurement-
/// libavif-mozjpeg.md` measured that AOM's and rav1e's quality scales are
/// calibrated *differently enough to reverse sign* at the same nominal
/// number - at quality 75, rav1e's DSSIM was **1.81x worse** than mozjpeg's
/// at matched nominal quality (`adr/0004-avif-measurement.md`, the figure
/// ADR 0005 reproduces as void-but-informative), while AOM's was **0.63x**
/// (better) at that same nominal number (`adr/0005`, "The nominal-quality
/// trap got worse, and reversed sign"). There is no published, measured
/// formula this module can apply to correct for that gap - inventing one
/// would be exactly the "naive same-nominal-quality" comparison both ADRs
/// warn is meaningless. So the number is passed through honestly instead:
/// `DEFAULT_AVIF_QUALITY`/`DEFAULT_AVIF_SPEED` (`handler.rs`) were
/// calibrated for AOM and are now, again, being handed to rav1e verbatim -
/// **`TODO(re-measure)`**: re-run an ADR-0005-style DSSIM/size/time sweep
/// against this `ravif`-backed encoder specifically (as ADR 0004 originally
/// did, before #67/#68) and re-derive both constants' values from that, not
/// from this comment's numbers.
///
/// # Chroma: 4:4:4, not 4:2:0 - a real output-bytes change
///
/// `ravif::ColorModel`'s own doc comment states this library "always uses
/// full-resolution color (4:4:4)" - there is no subsampling knob. The old
/// AOM path encoded 4:2:0 (`avif_codec.rs`'s prior `AVIF_PIXEL_FORMAT_
/// YUV420`, libavif/`avifenc`'s own default for lossy photographic
/// content). 4:4:4 is real signal (no chroma information is discarded) at
/// a real file-size cost versus 4:2:0 for the same nominal quality - a
/// second, independent reason `quality`'s meaning has moved, on top of the
/// scale-calibration gap above.
///
/// Alpha quality is set equal to `quality`, matching the old behaviour -
/// this crate's request surface has no separate alpha-quality knob.
///
/// # EXIF (#5): written, not a no-op - unlike decode (see `decode_inner`)
///
/// `ravif::Encoder::with_exif` **is** a real EXIF-write API (`exif_slice`
/// is "Embedded into AVIF file as-is", dropped into the MPEG `infe` box per
/// its own doc comment) - so, unlike decode's capability loss, encode-side
/// EXIF survives this swap intact, matching what the old
/// `avifImageSetMetadataExif` call provided. ICC remains intentionally not
/// threaded through here, same as before this change (see
/// `encode_single_image`'s own ICC comment in `handler.rs`) -
/// `avif-serialize` (which `ravif` writes through) does have an ICC-profile
/// slot, and could carry it in a future change, but that's new scope this
/// change doesn't take on.
///
/// `pub` (matching `encode_webp`/`encode_jpeg` in `handler.rs`) so
/// `benches/encode.rs` can benchmark the exact path production uses.
pub fn encode(
    img: &DynamicImage,
    quality: u8,
    speed: u8,
    exif_metadata: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let rgba = img.to_rgba8();

    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        encode_inner(&rgba, quality, speed, exif_metadata)
    }))
    .unwrap_or_else(|payload| Err(panic_to_error(payload, "ravif encode")))
}

fn encode_inner(
    rgba: &image::RgbaImage,
    quality: u8,
    speed: u8,
    exif_metadata: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let (width, height) = rgba.dimensions();
    anyhow::ensure!(
        width > 0 && height > 0,
        "ravif: cannot encode a zero-sized image"
    );

    let quality = f32::from(quality).clamp(1.0, 100.0);
    let speed = speed.clamp(1, 10);

    let mut encoder = ravif::Encoder::new()
        .with_quality(quality)
        .with_alpha_quality(quality)
        .with_speed(speed);
    if let Some(exif) = exif_metadata.filter(|e| !e.is_empty()) {
        encoder = encoder.with_exif(exif);
    }

    // `encode_rgba` inspects the actual pixel data
    // (`buffer.pixels().any(|px| px.a != 255)`) to decide whether an alpha
    // plane is worth encoding at all - no separate "does this image have
    // alpha" flag needs to be threaded in the way libavif's
    // `avifRGBImage.ignoreAlpha` required, since `img.to_rgba8()` above
    // already normalises every source (alpha or not) to the same RGBA8
    // shape `ravif` expects.
    use rgb::FromSlice;
    let pixels = rgba.as_raw().as_rgba();
    let buffer = imgref::Img::new(pixels, width as usize, height as usize);

    let encoded = encoder
        .encode_rgba(buffer)
        .map_err(|e| anyhow::anyhow!("ravif: encode failed: {e}"))?;

    Ok(encoded.avif_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    /// `handler.rs::ImageService::is_avif` is a hand-rolled, parser-free
    /// `ftyp`-box magic-byte check (kept dependency-free there so
    /// `detect_format_from_bytes` doesn't need to link an AVIF parser just
    /// to sniff a format tag - see that function's own doc comment). This
    /// pins it against `avif_parse::read_avif` succeeding (this module's
    /// `is_avif` - see that function's own doc comment on what changed here
    /// post-libavif) on a real encoded AVIF, so the two can't silently
    /// drift apart.
    #[test]
    fn handler_is_avif_agrees_with_avif_parse_read_avif() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4));
        let avif_bytes = encode(&img, 50, 8, None).expect("AVIF encode should succeed");

        assert!(is_avif(&avif_bytes));
        assert!(crate::services::image::handler::ImageService::is_avif(
            &avif_bytes
        ));

        // Neither should misdetect an unrelated format.
        let not_avif = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00garbage!!!!";
        assert!(!is_avif(not_avif));
        assert!(!crate::services::image::handler::ImageService::is_avif(
            not_avif
        ));
    }

    #[test]
    fn encode_then_decode_round_trips_dimensions() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            24,
            image::Rgb([120, 60, 200]),
        ));
        let avif_bytes = encode(&img, 70, 8, None).expect("AVIF encode should succeed");

        let (decoded, orientation, _icc, _exif) =
            decode(&avif_bytes, 50).expect("AVIF decode should succeed");

        assert_eq!(decoded.dimensions(), (32, 24));
        // Always `NoTransforms` now - `avif-parse` has no `irot`/`imir`
        // support to read a different value from. See `decode_inner`'s own
        // doc comment.
        assert_eq!(orientation, Orientation::NoTransforms);
    }

    #[test]
    fn peek_dimensions_matches_decoded_dimensions() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            17,
            9,
            image::Rgb([10, 20, 30]),
        ));
        let avif_bytes = encode(&img, 70, 8, None).expect("AVIF encode should succeed");

        let peeked = peek_dimensions(&avif_bytes).expect("peek_dimensions should succeed");
        assert_eq!(peeked, (17, 9));
    }

    #[test]
    fn decode_rejects_oversized_source_before_the_expensive_decode() {
        // 1200x900 = 1_080_000 px = 1 MP once `check_source_resolution`'s
        // own integer-truncating `pixels / 1_000_000` division rounds it
        // down - comfortably over a max of 0 MP. A smaller image wouldn't
        // trip this guard at all: `check_source_resolution` truncates to
        // whole megapixels, so anything under 1 MP computes `megapixels =
        // 0`, which never exceeds a `max_src_resolution_mp` of `0` either.
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            1200,
            900,
            image::Rgb([1, 2, 3]),
        ));
        let avif_bytes = encode(&img, 50, 10, None).expect("AVIF encode should succeed");

        let err = decode(&avif_bytes, 0).expect_err("oversized source should be rejected");
        assert!(
            err.to_string().contains("resolution too large"),
            "unexpected error: {err}"
        );
    }

    /// `ravif::Encoder::with_exif` writes a real EXIF item into the AVIF
    /// file (asserted here by checking the raw payload is present in the
    /// output bytes), but `avif-parse` - and so this module's `decode` -
    /// has no EXIF-read support at all (see `decode_inner`'s own doc
    /// comment on why). This pins both halves of that asymmetry: encode
    /// writes it, decode cannot read it back.
    #[test]
    fn encode_writes_exif_but_decode_cannot_read_it_back() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4));
        // Minimal well-formed EXIF TIFF header (no IFD entries).
        let exif: &[u8] = b"II*\0\x08\0\0\0\0\0\0\0";
        let avif_bytes = encode(&img, 70, 8, Some(exif)).expect("AVIF encode should succeed");

        let contains_exif_payload = avif_bytes.windows(exif.len()).any(|w| w == exif);
        assert!(
            contains_exif_payload,
            "encoded AVIF should embed the EXIF payload as-is"
        );

        let (_decoded, _orientation, _icc, exif_out) =
            decode(&avif_bytes, 50).expect("AVIF decode should succeed");
        assert_eq!(
            exif_out, None,
            "avif-parse has no EXIF-read support - see decode_inner's doc comment"
        );
    }
}
