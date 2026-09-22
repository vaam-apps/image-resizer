//! Decode-stage benchmarks: JPEG / PNG / WebP / AVIF, decoding a fixed,
//! checked-in corpus of real photographs (`benches/fixtures/decode/`, #139).
//!
//! # Why a fixed corpus, not fixtures generated at run time
//!
//! This bench used to encode its own fixtures at run time, with the encoder
//! under test. When that encoder changed, the two runs decoded *different
//! bitstreams*, so the comparison measured the encoder's output, not decoder
//! speed. That is not hypothetical: after the codec swap in #134 this bench
//! appeared to show AVIF decode at **0.20x** (a 5x speedup), which was
//! meaningless - the fixtures had silently moved from AOM 4:2:0 to ravif
//! 4:4:4 between the two runs being compared. `adr/0006-pure-rust-codecs.md`
//! records that episode and the honest like-for-like numbers that replaced
//! it.
//!
//! The corpus in `benches/fixtures/decode/` is produced once, by third-party
//! reference encoders, and checked in - it never changes, so a decode number
//! from this bench means the same thing in six months as it does today. See
//! that directory's `README.md` for full provenance, licensing and
//! regeneration instructions. Do not regenerate or edit those files from
//! this bench.
//!
//! # Which decode path each format exercises
//!
//! JPEG calls `ImageService::decode_with_image_crate`, NOT
//! `jpeg_scaled_decode`. This bench requests no resize, so
//! `select_jpeg_dct_scale` returns `scale_num == 8` ("no DCT reduction"), and
//! `decode_jpeg_scaled` routes that case to the `image`-crate/zune-jpeg
//! decoder rather than `jpeg-decoder` - zune is ~1.9x faster when there is no
//! reduction to exploit, which is worth 15% of a whole no-resize request.
//! Calling `jpeg_scaled_decode(&bytes, 8)` here, as this bench used to, sits
//! *below* that branch and therefore measures a decoder production does not
//! use for this shape of request. `jpeg_scaled_decode` is still the right
//! call for a bench that exercises a real DCT reduction (scale_num < 8);
//! there is not one here yet.
//!
//! PNG calls `image::load_from_memory_with_format`, the exact call
//! `ImageService::decode_with_image_crate` (src/services/image/handler.rs)
//! makes for that format.
//!
//! WebP calls `ImageService::decode_webp_pixels`, the pure-Rust decoder in
//! `vaam-image-webp`. `image`'s own WebP feature is not enabled, so
//! `load_from_memory_with_format` cannot reach a WebP decoder at all.
//!
//! AVIF calls `avif_codec::decode` - `rav1d` via `avif-decode`, this crate's
//! only AVIF decode path (`image`'s own decoder needs the separate
//! `avif-native` feature, not enabled - see `avif_codec`'s own module doc
//! comment).

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use emgr::services::image::avif_codec;
use emgr::services::image::handler::ImageService;
use image::ImageFormat;
use std::path::Path;

/// The two source photographs in the corpus - see
/// `benches/fixtures/decode/README.md` for why two, and why these two.
const PHOTOS: [&str; 2] = ["blue-marble", "earthrise"];

/// Sizes available for the lossy formats (jpg/webp/avif). PNG is only
/// present at the first size - see the README's "Contents" table.
const LOSSY_SIZES: [&str; 2] = ["640x360", "1280x720"];

/// Read a fixture out of the checked-in corpus. Panics with the path it
/// looked for rather than skipping - a missing fixture should fail the bench
/// run loudly, not silently shrink the benchmark group.
fn fixture(photo: &str, size: &str, ext: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benches/fixtures/decode")
        .join(format!("{photo}_{size}.{ext}"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing decode fixture at {}: {e} (see benches/fixtures/decode/README.md)",
            path.display()
        )
    })
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");

    for size in LOSSY_SIZES {
        for photo in PHOTOS {
            let bytes = fixture(photo, size, "jpg");
            group.throughput(Throughput::Bytes(bytes.len() as u64));
            group.bench_with_input(
                BenchmarkId::new("jpeg", format!("{photo}/{size}")),
                &bytes,
                |b, bytes| {
                    // Matches production for a no-resize request - see this
                    // module's doc comment.
                    b.iter(|| {
                        ImageService::decode_with_image_crate(bytes, None, u64::MAX, false)
                            .expect("decode fixture")
                    });
                },
            );
        }
    }

    for photo in PHOTOS {
        let bytes = fixture(photo, "640x360", "png");
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("png", format!("{photo}/640x360")),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    image::load_from_memory_with_format(bytes, ImageFormat::Png)
                        .expect("decode fixture")
                });
            },
        );
    }

    for size in LOSSY_SIZES {
        for photo in PHOTOS {
            let bytes = fixture(photo, size, "webp");
            group.throughput(Throughput::Bytes(bytes.len() as u64));
            group.bench_with_input(
                BenchmarkId::new("webp", format!("{photo}/{size}")),
                &bytes,
                |b, bytes| {
                    b.iter(|| ImageService::decode_webp_pixels(bytes).expect("decode fixture"));
                },
            );
        }
    }

    for size in LOSSY_SIZES {
        for photo in PHOTOS {
            let bytes = fixture(photo, size, "avif");
            group.throughput(Throughput::Bytes(bytes.len() as u64));
            group.bench_with_input(
                BenchmarkId::new("avif", format!("{photo}/{size}")),
                &bytes,
                |b, bytes| {
                    b.iter(|| avif_codec::decode(bytes, 50).expect("decode fixture"));
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
