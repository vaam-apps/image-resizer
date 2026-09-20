# License

`emgr` is licensed under the MIT License. The full text below is
reproduced from the repository's [`LICENSE`](https://github.com/vaam-store/image-resizer/blob/main/LICENSE)
file.

```text
MIT License

Copyright (c) 2025 Stephane SEGNING LAMBOU

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Third-party notices

`emgr` has no C or C++ dependencies (C-dependency removal) — every image codec it uses is
implemented in Rust and statically linked into the binary. That did not
clear every third-party attribution obligation, though: one licence
survived the rewrite. Reproduced from the repository's
[`NOTICE`](https://github.com/vaam-store/image-resizer/blob/main/NOTICE)
file:

- **jpeg-encoder** (`(MIT OR Apache-2.0) AND IJG`) is used for JPEG
  encoding, replacing `mozjpeg`/`mozjpeg-sys` (which vendored libjpeg-turbo
  and Independent JPEG Group (IJG) code - see the
  [changelog](changelog.md#performance)). It is easy to assume rewriting a
  codec in Rust sheds a C library's licence, but `jpeg-encoder` is itself
  licensed `(MIT OR Apache-2.0) AND IJG`: its default quantisation and
  Huffman tables derive from the IJG reference implementation. Per the IJG
  license, this software is based in part on the work of the Independent
  JPEG Group, and that notice stays required for as long as `jpeg-encoder`
  is a dependency.
- **jpeg-decoder** (`MIT OR Apache-2.0`) is used for DCT-scaled and
  full-size JPEG decoding, also replacing `mozjpeg`/`mozjpeg-sys`. No IJG
  obligation attaches to this crate.
- **vaam-image-webp** (`MIT OR Apache-2.0`), this org's fork of
  `image-rs/image-webp`, is used for WebP encoding and decoding, replacing
  the `webp` crate (real libwebp via FFI).
- **ravif** + **rav1e** (`BSD-2-Clause`) is used for AVIF encoding,
  replacing `libavif`/AOM.
- **avif-decode** + **rav1d** (`BSD-2-Clause`) is used for AVIF decoding,
  replacing `libavif`/dav1d. `rav1d` is a Rust port of `dav1d` and carries
  its BSD-2-Clause license.

Full license texts for every dependency ship with those crates and are
reproduced in the dependency tree under `~/.cargo/registry` (or your
project's vendored `Cargo.lock`-resolved sources). `cargo-deny`'s
`licenses` check (`deny.toml`, run in CI) enforces that every dependency's
declared license is one this project allows - see
[Contributing](../development/contributing.md) for running it locally.
