# Fixed decode corpus

Pre-encoded images for `benches/decode.rs`, checked in so the decode benchmarks
stay comparable across commits (#139).

## Why these are files rather than generated

The decode benches used to encode their own fixtures at run time, with the
encoder under test. When that encoder changed, the two runs decoded *different
bitstreams*, so the comparison measured content rather than decoder speed.

That is not hypothetical. After the codec swap in #134 those benches appeared to
show AVIF decode at **0.20x** — a 5x speedup — which was meaningless: the
fixtures had moved from AOM 4:2:0 to ravif 4:4:4. `adr/0006-pure-rust-codecs.md`
records the episode and the honest like-for-like table that replaced it.

These files never change, so a decode number from this bench means the same
thing in six months as it does today.

## Why third-party encoders

Every lossy file here was produced by the **reference C encoders**, not by this
service. That is deliberate twice over:

1. It keeps the corpus independent of our encoder entirely, so no future
   encoder change can move it.
2. It is what production actually decodes. This is an image proxy — sources are
   fetched from arbitrary URLs and were encoded by somebody else, overwhelmingly
   by libwebp, libavif/AOM or a camera's JPEG encoder. Benchmarking our decoder
   against our own encoder's output would measure an input shape we rarely see.

## Provenance

Source images are the two public-domain NASA photographs already in
`benches/fixtures/real/` — see `../real/ATTRIBUTION.md` for their origin and
licence. They are centre-cropped to exact dimensions, so every format at a given
size decodes the identical picture and the formats stay readable against each
other.

Regenerate with, from the repository root:

```bash
for f in blue-marble earthrise; do
  for wh in 640x360 1280x720; do
    w=${wh%x*}; h=${wh#*x}
    sips -s format png -c "$h" "$w" "benches/fixtures/real/$f.jpg" --out "/tmp/${f}_${wh}.png"
  done
done

cd /tmp
for p in *_640x360.png *_1280x720.png; do
  b=${p%.png}
  cwebp -q 80 -quiet "$p" -o "$b.webp"
  avifenc -q 60 -s 6 "$p" "$b.avif"
  sips -s format jpeg -s formatOptions 75 "$p" --out "$b.jpg"
done
```

Produced with `cwebp 1.6.0` (libwebp) and `avifenc` (libavif/AOM). Exact
encoder versions are not load-bearing — the committed bytes are what matter, and
they are fixed. Regeneration is for adding images, not for reproducing these.

## Contents

| | 640x360 | 1280x720 |
|---|---|---|
| `.jpg` | both photos | both photos |
| `.webp` | both photos | both photos |
| `.avif` | both photos | both photos |
| `.png` | both photos | — |

PNG is present only at 640x360. It is lossless, so a 1280x720 photo costs
~730 KB against ~60 KB for the lossy formats, and PNG decode scales predictably
enough that the second size does not earn that. PNG is the control here anyway:
it is the one format this service never changed.

Two photographs rather than one, because they differ in a way that matters for a
decoder — `blue-marble` is high-entropy detail over most of its area, while
`earthrise` is largely flat black with a small bright subject, which exercises
very different coefficient distributions.

No synthetic gradient+noise fixture is included, deliberately. ADRs 0001, 0003,
0004, 0005 and 0006 all record that trap: noise compresses to a shared
incompressible floor and distorts both size and decode-speed conclusions. The
encode benches still cover synthetic content, where a worst case is genuinely
informative.
