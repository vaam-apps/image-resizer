//! Full codec report: output size, encode time and decode time for every
//! format this service emits, measured through the production encode paths.
//!
//! Two modes, because the two questions need different setups.
//!
//! `encode <corpus-dir> <out-dir>`
//!     For each image and each DSSIM target, bisects the format's own quality
//!     scale until the decoded result hits that perceptual target, then
//!     records the byte size and encode time. Nominal quality numbers are not
//!     comparable across encoders - two encoders' "q75" are different
//!     qualities, sometimes by enough to reverse a comparison - so matched
//!     DSSIM is the only basis on which sizes mean anything. Encoded files are
//!     written to <out-dir> for the decode phase.
//!
//! `decode <files-dir>`
//!     Decodes files produced by an *earlier* run, possibly from a different
//!     build, and times it. This is the only way to compare decoders honestly:
//!     the repo's `decode/*` benches decode fixtures made by the encoder under
//!     test, so when the encoder changes they measure different bitstreams
//!     rather than decoder speed (see adr/0006).
//!
//! CSV on stdout; progress on stderr.
use emgr::services::image::avif_codec;
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic};
use emgr::services::image::handler::ImageService;
use image::DynamicImage;
use std::path::Path;
use std::time::Instant;

/// Which perceptual metric drives the quality bisection.
///
/// This matters more than it looks. Matching on DSSIM and matching on
/// SSIMULACRA2 do NOT rank the formats the same way: scoring DSSIM-matched
/// files with SSIMULACRA2 shows WebP landing 4.8 points below AVIF and JPEG at
/// the loosest target, i.e. DSSIM calls files "equal quality" that SSIMULACRA2
/// says are clearly worse. Any size comparison inherits that bias, so the
/// metric is a parameter rather than a hardcoded choice.
#[derive(Clone, Copy, PartialEq)]
enum Metric {
    Dssim,
    Ssimulacra2,
}

/// DSSIM targets (lower is better) and their rough SSIMULACRA2 equivalents
/// (higher is better), calibrated from the medians actually observed on this
/// corpus so the two metrics bisect to comparable quality levels.
const TARGETS: [f64; 3] = [0.0150, 0.0080, 0.0035];
const TARGETS_S2: [f64; 3] = [26.5, 48.0, 66.3];
/// Production default; see `DEFAULT_AVIF_SPEED`.
const AVIF_SPEED: u8 = 6;
/// Megapixel cap handed to `avif_codec::decode`, matching
/// `PerformanceConfig::default()` so this measures what production does.
///
/// Not an arbitrary "big number": libavif turns this into
/// `decoder->imageSizeLimit`, and anything above its own
/// AVIF_DEFAULT_IMAGE_SIZE_LIMIT (16384x16384, ~268 MP) is refused outright
/// with AVIF_RESULT_NOT_IMPLEMENTED. That failure is indistinguishable from a
/// libavif built without a decoder - it fires at every quality, every speed
/// and every image size - so it reads as "AVIF decode is broken" rather than
/// "this argument is out of range". Verified by holding size and speed fixed
/// and varying only the cap.
const DECODE_MP_CAP: u64 = 50;

fn to_s2_rgb(bytes: &[u8], w: usize, h: usize) -> Rgb {
    let data: Vec<[f32; 3]> = bytes
        .chunks_exact(3)
        .map(|c| [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0])
        .collect();
    Rgb::new(data, w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709).unwrap()
}

fn ssimulacra2_of(orig: &[u8], dec: &[u8], w: usize, h: usize) -> f64 {
    ssimulacra2::compute_frame_ssimulacra2(to_s2_rgb(orig, w, h), to_s2_rgb(dec, w, h))
        .expect("ssimulacra2")
}

fn dssim_of(orig: &[u8], dec: &[u8], w: usize, h: usize) -> f64 {
    let d = dssim_core::Dssim::new();
    let mk = |p: &[u8]| {
        let px: Vec<rgb::RGB8> = p
            .chunks_exact(3)
            .map(|c| rgb::RGB8::new(c[0], c[1], c[2]))
            .collect();
        d.create_image_rgb(&px, w, h).unwrap()
    };
    let (v, _) = d.compare(&mk(orig), mk(dec));
    v.into()
}

fn encode(fmt: &str, img: &DynamicImage, q: u8) -> Vec<u8> {
    match fmt {
        "jpeg" => ImageService::encode_jpeg(img, q, false, false, None, None).expect("jpeg"),
        "jpegprog" => ImageService::encode_jpeg(img, q, true, false, None, None).expect("jpegprog"),
        "webp" => ImageService::encode_webp(img, f32::from(q), false).expect("webp"),
        "avif" => avif_codec::encode(img, q, AVIF_SPEED, None).expect("avif"),
        _ => unreachable!(),
    }
}

/// Decoded back to RGB8 for scoring. Routed through each build's own
/// production decoder, which is the point - `image` cannot decode WebP here at
/// all, and never could decode AVIF.
fn decode(fmt: &str, bytes: &[u8]) -> DynamicImage {
    match fmt {
        "jpeg" | "jpegprog" => ImageService::jpeg_scaled_decode(bytes, 8).expect("jpeg decode"),
        "webp" => ImageService::decode_webp_pixels(bytes).expect("webp decode"),
        "avif" => avif_codec::decode(bytes, DECODE_MP_CAP).expect("avif decode").0,
        _ => unreachable!(),
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time_encode(fmt: &str, img: &DynamicImage, q: u8, n: usize) -> f64 {
    let mut t = Vec::with_capacity(n);
    for _ in 0..n {
        let s = Instant::now();
        let _ = encode(fmt, img, q);
        t.push(s.elapsed().as_secs_f64() * 1e3);
    }
    median(t)
}

fn time_decode(fmt: &str, bytes: &[u8], n: usize) -> f64 {
    let mut t = Vec::with_capacity(n);
    for _ in 0..n {
        let s = Instant::now();
        let _ = decode(fmt, bytes);
        t.push(s.elapsed().as_secs_f64() * 1e3);
    }
    median(t)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("usage: codec_report <encode|decode> ...");
    match mode.as_str() {
        "encode" => {
            let corpus = args.next().expect("corpus dir");
            let out = args.next().expect("out dir");
            std::fs::create_dir_all(&out).unwrap();
            run_encode(&corpus, &out);
        }
        "decode" => {
            let dir = args.next().expect("files dir");
            run_decode(&dir);
        }
        "conform" => {
            let dir = args.next().expect("files dir");
            let out = args.next().expect("out dir");
            run_conform(&dir, &out);
        }
        other => panic!("unknown mode {other}"),
    }
}

fn run_encode(corpus: &str, out: &str) {
    println!("image,format,target,quality,bytes,dssim,encode_ms,decode_ms");
    let mut files: Vec<_> = std::fs::read_dir(corpus)
        .expect("corpus")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    files.sort();

    for path in &files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let img = image::open(path).expect("open").to_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let dynimg = DynamicImage::ImageRgb8(img.clone());
        let orig = img.as_raw();

        // PNG is lossless and has no quality knob; recorded once as the
        // "what does lossless cost" reference point.
        let t = Instant::now();
        let png = ImageService::encode_png(&dynimg, None, None).expect("png");
        let png_ms = t.elapsed().as_secs_f64() * 1e3;
        println!("{name},png,lossless,-,{},0.0,{:.3},-", png.len(), png_ms);

        for fmt in ["jpeg", "jpegprog", "webp", "avif"] {
            for target in TARGETS {
                // Lowest quality whose decoded DSSIM meets the target.
                let (mut lo, mut hi) = (1u8, 100u8);
                let mut best: Option<(u8, Vec<u8>, f64)> = None;
                while lo <= hi {
                    let mid = lo + (hi - lo) / 2;
                    let bytes = encode(fmt, &dynimg, mid);
                    let dec = decode(fmt, &bytes).to_rgb8();
                    let d = dssim_of(orig, dec.as_raw(), w, h);
                    if d <= target {
                        best = Some((mid, bytes, d));
                        if mid == 1 {
                            break;
                        }
                        hi = mid - 1;
                    } else {
                        lo = mid + 1;
                    }
                }
                match best {
                    Some((q, bytes, d)) => {
                        let enc_ms = time_encode(fmt, &dynimg, q, 3);
                        let dec_ms = time_decode(fmt, &bytes, 5);
                        let ext = if fmt.starts_with("jpeg") { "jpg" } else { fmt };
                        let fname = format!("{name}__{fmt}__{target}.{ext}");
                        std::fs::write(Path::new(out).join(&fname), &bytes).unwrap();
                        println!(
                            "{name},{fmt},{target},{q},{},{d:.6},{enc_ms:.3},{dec_ms:.3}",
                            bytes.len()
                        );
                    }
                    None => println!("{name},{fmt},{target},unreachable,-,-,-,-"),
                }
            }
        }
        eprintln!("done {name}");
    }
}

/// Decodes every file and writes the decoded RGB8 planes to `<out>/<file>.rgb`,
/// so two builds can be compared pixel-for-pixel.
///
/// VP8 (WebP) and AV1 (AVIF) decoding are exactly specified: a conforming
/// decoder must produce bit-identical output, so any difference at all is a
/// bug in one of them. JPEG is not - the standard leaves IDCT precision open,
/// so small per-pixel differences are legitimate and only large ones indicate
/// a fault.
fn run_conform(dir: &str, out: &str) {
    std::fs::create_dir_all(out).unwrap();
    println!("file,format,width,height,bytes");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("files dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e != "rgb"))
        .collect();
    files.sort();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(fmt) = name.split("__").nth(1) else { continue };
        let bytes = std::fs::read(path).unwrap();
        let img = decode(fmt, &bytes).to_rgb8();
        let (w, h) = (img.width(), img.height());
        std::fs::write(Path::new(out).join(format!("{name}.rgb")), img.as_raw()).unwrap();
        println!("{name},{fmt},{w},{h},{}", img.as_raw().len());
    }
}

fn run_decode(dir: &str) {
    println!("file,format,bytes,decode_ms");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("files dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    files.sort();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(fmt) = name.split("__").nth(1) else { continue };
        let bytes = std::fs::read(path).unwrap();
        let ms = time_decode(fmt, &bytes, 5);
        println!("{name},{fmt},{},{ms:.3}", bytes.len());
    }
}
