# Changelog

## [0.2.2](https://github.com/vaam-apps/image-resizer/compare/v0.2.1...v0.2.2) (2026-09-19)


### Bug Fixes

* **ci:** scan the default branch on push, not just pull_request ([#119](https://github.com/vaam-apps/image-resizer/issues/119)) ([73d93fe](https://github.com/vaam-apps/image-resizer/commit/73d93fe1378a44aa1c35f48e9c7077c4dc957443))

## [0.2.1](https://github.com/vaam-apps/image-resizer/compare/v0.2.0...v0.2.1) (2026-09-19)


### Chores

* **deps:** bump axum-tracing-opentelemetry from 0.29.0 to 0.39.1 ([#102](https://github.com/vaam-apps/image-resizer/issues/102)) ([2bb8adc](https://github.com/vaam-apps/image-resizer/commit/2bb8adce234b26695107dcf83e0213283fc1b7bf))
* **deps:** bump tracing-opentelemetry from 0.30.0 to 0.33.0 ([#108](https://github.com/vaam-apps/image-resizer/issues/108)) ([11111c3](https://github.com/vaam-apps/image-resizer/commit/11111c3b37ebdad3f80e4905a37cbb22131b418b))

## [0.2.0](https://github.com/vaam-apps/image-resizer/compare/v0.1.2...v0.2.0) (2026-09-19)


### Features

* add cache control headers to image download ([79a9e60](https://github.com/vaam-apps/image-resizer/commit/79a9e60d28ec863d5c1417c4412e3e3565235e45))
* add ETag and further Cache-Control headers to API ([b3e13d9](https://github.com/vaam-apps/image-resizer/commit/b3e13d9780ece0c365d0e6fc203fb6f7c936b774))
* add ETag and further Cache-Control headers to API ([03c4621](https://github.com/vaam-apps/image-resizer/commit/03c4621346c90ceee7c674799003e86c480a4ced))
* add Knative Serverless Helm chart for image-resizer deployment ([bc8f57a](https://github.com/vaam-apps/image-resizer/commit/bc8f57ad36a84284daf658f9cf3f3bcce7c189ba))
* Add S3 storage support with subpath functionality ([2b4477f](https://github.com/vaam-apps/image-resizer/commit/2b4477f516e07dca4c6d9c0ccda8e749055f9c7c))
* apply EXIF orientation and forward ICC profiles ([#33](https://github.com/vaam-apps/image-resizer/issues/33), [#5](https://github.com/vaam-apps/image-resizer/issues/5)) ([9b1c8b4](https://github.com/vaam-apps/image-resizer/commit/9b1c8b4f01e77fbe3f4f23866083e9a0fb8b4ff1))
* apply EXIF orientation and forward ICC profiles ([#33](https://github.com/vaam-apps/image-resizer/issues/33), [#5](https://github.com/vaam-apps/image-resizer/issues/5)) ([6d9b81a](https://github.com/vaam-apps/image-resizer/commit/6d9b81a90b1a9ca4f8a1d65b6c3e61c154d857e8))
* AVIF, animation, gravity, geometry, watermarks and presets ([#49](https://github.com/vaam-apps/image-resizer/issues/49), [#50](https://github.com/vaam-apps/image-resizer/issues/50), [#51](https://github.com/vaam-apps/image-resizer/issues/51), [#52](https://github.com/vaam-apps/image-resizer/issues/52)) ([41aee92](https://github.com/vaam-apps/image-resizer/commit/41aee92bd1c702ae83f3bcc35a41ca2b99d80bf1))
* AVIF, animation, gravity, geometry, watermarks and presets ([#49](https://github.com/vaam-apps/image-resizer/issues/49), [#50](https://github.com/vaam-apps/image-resizer/issues/50), [#51](https://github.com/vaam-apps/image-resizer/issues/51), [#52](https://github.com/vaam-apps/image-resizer/issues/52)) ([a88084a](https://github.com/vaam-apps/image-resizer/commit/a88084a1975da9a326570ae5608dede106955d0d))
* basic layout ([4c8d504](https://github.com/vaam-apps/image-resizer/commit/4c8d5044b051137a1163483bdaf8b26b27ccb9ef))
* benchmark foundation and P0 security wave (epic [#9](https://github.com/vaam-apps/image-resizer/issues/9)) ([4c4f155](https://github.com/vaam-apps/image-resizer/commit/4c4f155e8b711310de7833e325459d209eac8f35))
* **benchmark:** make benchmark configurable using environment variables ([befbea2](https://github.com/vaam-apps/image-resizer/commit/befbea2bedaff170c16ee0cb6c7f0cf38f1642e2))
* better readme ([87540f3](https://github.com/vaam-apps/image-resizer/commit/87540f3832e4a5cbf24c452c037bbef56baf6ed9))
* caller-controlled EXIF metadata strip/keep ([#5](https://github.com/vaam-apps/image-resizer/issues/5)) ([2c286a7](https://github.com/vaam-apps/image-resizer/commit/2c286a7280f782257da427464637717101b60aad))
* chart folder ([9a821ad](https://github.com/vaam-apps/image-resizer/commit/9a821ad5256b98f602aa46b7b1f111c4d6b23bad))
* chart folder ([83fb8f1](https://github.com/vaam-apps/image-resizer/commit/83fb8f18fd677c83aef0bff7941122ab8d136e89))
* chart folder ([75e7dc8](https://github.com/vaam-apps/image-resizer/commit/75e7dc8e03f93ed7b391adfb861581c5ca3f6c88))
* chart folder ([f9e0822](https://github.com/vaam-apps/image-resizer/commit/f9e08220de703ded41cced5731e20b9dfb21c0d1))
* chart folder ([18082dd](https://github.com/vaam-apps/image-resizer/commit/18082ddcf23addfccd4d52ffe13e21e50b9fefe6))
* chart folder ([68c8413](https://github.com/vaam-apps/image-resizer/commit/68c84135c1b02557fb2443d95ec5f0d229b1d9db))
* chart folder ([4d5840f](https://github.com/vaam-apps/image-resizer/commit/4d5840f3ac47733f5c177bece8d68e2840ce5776))
* ci ([8c14882](https://github.com/vaam-apps/image-resizer/commit/8c148823399fee041c97242a6fa7de7da2bdc6ec))
* ci ([0ac1048](https://github.com/vaam-apps/image-resizer/commit/0ac1048b0ef5447adb3bf4c1170dfd5d6d6d0f86))
* ci cache ([58023fc](https://github.com/vaam-apps/image-resizer/commit/58023fc731d9b9a0480285439384494e3963d3db))
* container + License ([d5457d2](https://github.com/vaam-apps/image-resizer/commit/d5457d2a89d98e91a6311ad47896e05baf0c589f))
* full ist working ([3bb78af](https://github.com/vaam-apps/image-resizer/commit/3bb78afecf73fe013fee7726fa097429452ccddb))
* image download working ([ade1ddc](https://github.com/vaam-apps/image-resizer/commit/ade1ddcbdf953d04b5af25a5a98c769d41968761))
* Improve image processing functionality ([242a790](https://github.com/vaam-apps/image-resizer/commit/242a790b9c3923a4caa0ba7abefa6f14bb22ee24))
* in-memory storage ([f10d744](https://github.com/vaam-apps/image-resizer/commit/f10d744a595b161b53b4fcbc6540ea0da1ad6647))
* initial commit ([4ddceda](https://github.com/vaam-apps/image-resizer/commit/4ddceda8d50e1a38331db8f02a5e54a6cf544023))
* Introduce new image processing parameters and enhance documentation/deployment ([5c7d20a](https://github.com/vaam-apps/image-resizer/commit/5c7d20a66932f57a68bc93702ae7efe0875fbc35))
* Introduce new image processing parameters and enhance documentation/deployment ([4f28aff](https://github.com/vaam-apps/image-resizer/commit/4f28aff5c6f7bd3019d78defbd1f629a1c018830))
* libwebp decode, dav1d AVIF decode, libavif+AOM AVIF encode ([bc8e28c](https://github.com/vaam-apps/image-resizer/commit/bc8e28cea346573ae8afab63df9a35e27979e68f))
* lossy WebP, signed URLs, codegen removal, imgproxy benchmark harness ([fc8868e](https://github.com/vaam-apps/image-resizer/commit/fc8868e758a4dc279ad8d7fd8f74032a1e8aa3f1))
* merge performance config with environment variables ([277edcc](https://github.com/vaam-apps/image-resizer/commit/277edcc2c256e666600f193b0887bc9f0b798012))
* on error =&gt; url ([72f3bbb](https://github.com/vaam-apps/image-resizer/commit/72f3bbb21a40c89d99029e91fe25dcdae9886111))
* progressive JPEG, chroma subsampling and max_bytes ([#76](https://github.com/vaam-apps/image-resizer/issues/76)) ([#80](https://github.com/vaam-apps/image-resizer/issues/80)) ([ebeb98d](https://github.com/vaam-apps/image-resizer/commit/ebeb98de0efb6ab4f9f95d47e23b47fb9cf0a0a1))
* quality, per-format quality and lossless WebP ([#35](https://github.com/vaam-apps/image-resizer/issues/35), [#4](https://github.com/vaam-apps/image-resizer/issues/4)) ([c6d6872](https://github.com/vaam-apps/image-resizer/commit/c6d687286ab72bdc41c50f8c1f658d37b29cbbfb))
* quality, per-format quality and lossless WebP ([#35](https://github.com/vaam-apps/image-resizer/issues/35), [#4](https://github.com/vaam-apps/image-resizer/issues/4)) ([#70](https://github.com/vaam-apps/image-resizer/issues/70)) ([abd7030](https://github.com/vaam-apps/image-resizer/commit/abd70309b45a83bdc3aee0ae12a43af8657e02d3))
* refactor storage handlers and improve S3 compatibility across multiple components ([94be0bb](https://github.com/vaam-apps/image-resizer/commit/94be0bb1eb3329014a0f1a253fc065208da602e0))
* sbom & provenance ([fb901d8](https://github.com/vaam-apps/image-resizer/commit/fb901d8bb5ebc38f41d0e871f0f4c3a846873758))
* split into multiple logic files ([5f4a246](https://github.com/vaam-apps/image-resizer/commit/5f4a2466f87599968fd976c4570267be9070d140))
* typo ([645b944](https://github.com/vaam-apps/image-resizer/commit/645b94455e9af45c631dc2ab1b6a8a13c6466e13))
* update image resize functionality ([d5eaab3](https://github.com/vaam-apps/image-resizer/commit/d5eaab324943b2fc9853b7593da793d300818225))
* update project branding and increment version to 0.1.2 ([ff94564](https://github.com/vaam-apps/image-resizer/commit/ff945648802205c825d004dc6c0c7e54ef2ef201))
* upload successful ([eb8ccce](https://github.com/vaam-apps/image-resizer/commit/eb8ccce929b726c689f23b9b8f4a5fd91fa58a74))
* wave 2 — concurrency, cache lifecycle, hardening, CI and docs (epic [#9](https://github.com/vaam-apps/image-resizer/issues/9)) ([3c441b3](https://github.com/vaam-apps/image-resizer/commit/3c441b3a394c14828cbdc2c5efcec9c64b0c10a4))
* WebP decode via libwebp, AVIF decode+encode via libavif ([#67](https://github.com/vaam-apps/image-resizer/issues/67)/[#68](https://github.com/vaam-apps/image-resizer/issues/68)) ([47ecf50](https://github.com/vaam-apps/image-resizer/commit/47ecf506d79ff48038004a675c270092f471fd54))


### Bug Fixes

* add missing trailing newline and stop yamllint re-indenting valid config ([b26cf7b](https://github.com/vaam-apps/image-resizer/commit/b26cf7bd3fd759be132d94f192733194739375cd))
* atomic fixture writes, and a concurrency test that cannot race ([#90](https://github.com/vaam-apps/image-resizer/issues/90)) ([9584580](https://github.com/vaam-apps/image-resizer/commit/9584580af8461d1b3c653a33b0249ad085a8e750))
* benches/encode.rs PNG case measured Fast compression, not production's Best ([5fece32](https://github.com/vaam-apps/image-resizer/commit/5fece326e4daa733c66be538a52414c3ddb629c7))
* benchmark fairness — rate limiter, resize semantics, origin healthcheck ([013104b](https://github.com/vaam-apps/image-resizer/commit/013104bd7aaf391f9fc052b8f341b706bbc756db))
* **bench:** measure per-image latency, not per-request ([735dd05](https://github.com/vaam-apps/image-resizer/commit/735dd058c19244a5b4451693599dca3329b0b218))
* **bench:** measure per-image latency, not per-request ([2e02d8d](https://github.com/vaam-apps/image-resizer/commit/2e02d8d59085f444207aaf01a99726298afc93ba))
* close every open issue ([#5](https://github.com/vaam-apps/image-resizer/issues/5), [#56](https://github.com/vaam-apps/image-resizer/issues/56), [#83](https://github.com/vaam-apps/image-resizer/issues/83), [#84](https://github.com/vaam-apps/image-resizer/issues/84), [#85](https://github.com/vaam-apps/image-resizer/issues/85)) ([76ce1ca](https://github.com/vaam-apps/image-resizer/commit/76ce1cab441970bfcbd686b47f67d9ed6b9de397))
* commit the generated gen-server crate so CI and fresh clones build ([#45](https://github.com/vaam-apps/image-resizer/issues/45)) ([2a9c3a2](https://github.com/vaam-apps/image-resizer/commit/2a9c3a297bf06e13e4bdab3c927fbe39a9abe98f))
* composite alpha and normalise transparent pixels ([#34](https://github.com/vaam-apps/image-resizer/issues/34), [#60](https://github.com/vaam-apps/image-resizer/issues/60)) ([#66](https://github.com/vaam-apps/image-resizer/issues/66)) ([195a9b8](https://github.com/vaam-apps/image-resizer/commit/195a9b841295251abc25a6a341d41756ec54b292))
* **deps:** bump rustls to 0.23.45 for RUSTSEC-2026-0285 ([#114](https://github.com/vaam-apps/image-resizer/issues/114)) ([1504305](https://github.com/vaam-apps/image-resizer/commit/15043050a189a6c93d29325e9c990e386ca84f67))
* **deps:** two lockstep dependency breaks — main has not compiled since [#104](https://github.com/vaam-apps/image-resizer/issues/104)/[#106](https://github.com/vaam-apps/image-resizer/issues/106)/[#109](https://github.com/vaam-apps/image-resizer/issues/109) ([#116](https://github.com/vaam-apps/image-resizer/issues/116)) ([82c07b6](https://github.com/vaam-apps/image-resizer/commit/82c07b661bc95252a9afa76b9c700c2b2c4de8d5))
* **docker:** run as a non-root user (DS-0002) ([#112](https://github.com/vaam-apps/image-resizer/issues/112)) ([f9c0945](https://github.com/vaam-apps/image-resizer/commit/f9c0945bc98e33a27fbe39f12d49f4da86f38783))
* document wave-2 env vars and give the bench gate an absolute floor ([fb1b91f](https://github.com/vaam-apps/image-resizer/commit/fb1b91f201076536b0847ade9c15381a68b4597b))
* drop legacy TLS stack from aws-sdk-s3 to clear RUSTSEC-2026-0258/0098/0099/0104 ([5f28eb7](https://github.com/vaam-apps/image-resizer/commit/5f28eb7283c4b29ddc386a1841c6c699cb0d2079))
* drop the stale 'make init' step from the Docker build workflow ([2be8c08](https://github.com/vaam-apps/image-resizer/commit/2be8c0884726a81257d1b7ca31ad57d918621897))
* eliminate GH [#90](https://github.com/vaam-apps/image-resizer/issues/90) concurrency-test flake at its root cause ([616543d](https://github.com/vaam-apps/image-resizer/commit/616543d1994df076f06c4d13ee6972c250bd5107))
* enable_http2 no-profile fallback now matches Default::default() ([#83](https://github.com/vaam-apps/image-resizer/issues/83)) ([630ff26](https://github.com/vaam-apps/image-resizer/commit/630ff26308bfc9d73c9c58428acb0d88af12a745))
* exclude Helm templates from YAML linting at the super-linter level ([b3410c3](https://github.com/vaam-apps/image-resizer/commit/b3410c3f75305fefd719582726e00c6731412527))
* helm deploy ([022b764](https://github.com/vaam-apps/image-resizer/commit/022b764183000b7e6686a2a9657ceed31ac3098f))
* helm deploy v2 ([8463dc5](https://github.com/vaam-apps/image-resizer/commit/8463dc57e665616e8a37e0761623b89ce8c0ef16))
* helm deploy v3 ([7d77696](https://github.com/vaam-apps/image-resizer/commit/7d77696c24ff53104376d880150fecd4b7c4557d))
* helm deploy v4 ([43c3de3](https://github.com/vaam-apps/image-resizer/commit/43c3de32af4d415c30cfcb3eb63de86c3901d999))
* **helm:** mark METRICS_AUTH_TOKEN optional so non-otel pods can start ([76db770](https://github.com/vaam-apps/image-resizer/commit/76db770c2558eac8da79c18cf6c4160ff15810fa))
* honour imgproxy resize types instead of always cropping ([#59](https://github.com/vaam-apps/image-resizer/issues/59), [#1](https://github.com/vaam-apps/image-resizer/issues/1)) ([#64](https://github.com/vaam-apps/image-resizer/issues/64)) ([ffe8846](https://github.com/vaam-apps/image-resizer/commit/ffe8846392af6e62502092e93e3433f7d4312ea5))
* issue with router ([9564827](https://github.com/vaam-apps/image-resizer/commit/9564827a90da64533ad5bdcfba060bfbab6bce75))
* let ALLOWED_SOURCES authorise private origins ([#57](https://github.com/vaam-apps/image-resizer/issues/57)) ([#65](https://github.com/vaam-apps/image-resizer/issues/65)) ([08c874f](https://github.com/vaam-apps/image-resizer/commit/08c874fae8ca4dafebd15395bd5c0fd83e7a222f))
* make super-linter actually read the repo's linter configs ([c272bc3](https://github.com/vaam-apps/image-resizer/commit/c272bc304218bd9fc7607610de44f98421463b4d))
* pin loose dependency requirements and repair the otel feature ([7f7d34e](https://github.com/vaam-apps/image-resizer/commit/7f7d34e964e5d8433fef82f995a076a3be85a077))
* point the benchmark driver at emgr's signed-path API ([95c62c4](https://github.com/vaam-apps/image-resizer/commit/95c62c40b20fb215e04dd4bab7aa1b35741e4852))
* populate the empty .hadolint.yaml and .clippy.toml ([6fb038c](https://github.com/vaam-apps/image-resizer/commit/6fb038c70fcec3409e3f4755dd7db92869ec3c20))
* relax YAML line length and stop the bench gate failing without a baseline ([877ccdf](https://github.com/vaam-apps/image-resizer/commit/877ccdfb36a97b3ecc8ad5a255c17e941c8f4fe2))
* require a bearer token on /metrics, fail closed at startup ([#77](https://github.com/vaam-apps/image-resizer/issues/77)) ([#79](https://github.com/vaam-apps/image-resizer/issues/79)) ([ec9cad8](https://github.com/vaam-apps/image-resizer/commit/ec9cad8adff7645282c35236e70b5748cb98117b))
* s3 Docker images could not start; add three-way benchmark and S3 tests ([81a1dd9](https://github.com/vaam-apps/image-resizer/commit/81a1dd9455ebc6f7e0017dbf71bbae1ecc5562b6))
* strip trailing whitespace from openapi.yaml ([64cb0f0](https://github.com/vaam-apps/image-resizer/commit/64cb0f01f0a50e81571aa9b526e9f28ea53df617))
* unbreak Docker builds and satisfy cargo-deny under all features ([8593cb2](https://github.com/vaam-apps/image-resizer/commit/8593cb204b717bdb1b606870fc76107443317132))
* wire signing/metrics-auth env vars and fix helm/serverless rendering (GH [#84](https://github.com/vaam-apps/image-resizer/issues/84), [#85](https://github.com/vaam-apps/image-resizer/issues/85)) ([0d43ebf](https://github.com/vaam-apps/image-resizer/commit/0d43ebff329b679cb2f877d7121dfd83dc42e3ab))


### Performance

* DCT-scaled JPEG decode via mozjpeg ([#63](https://github.com/vaam-apps/image-resizer/issues/63) stage 2) ([3668b56](https://github.com/vaam-apps/image-resizer/commit/3668b56deb8107b918734332b5ea7f2a01d40990))
* DCT-scaled JPEG decode via mozjpeg ([#63](https://github.com/vaam-apps/image-resizer/issues/63) stage 2) ([9a6281c](https://github.com/vaam-apps/image-resizer/commit/9a6281cfddc1548b9110957e16c8d39273ef67a3))
* optimize image processing with tokio spawn_blocking ([66d829f](https://github.com/vaam-apps/image-resizer/commit/66d829fb188c98ad15e6deba16e433afb9f2a673))
* optimize performance configuration and settings ([7de96e7](https://github.com/vaam-apps/image-resizer/commit/7de96e741a478dc8ed83aa276a1e2947fe0082d3))
* route full-size JPEG decode through mozjpeg ([#67](https://github.com/vaam-apps/image-resizer/issues/67)) ([#81](https://github.com/vaam-apps/image-resizer/issues/81)) ([e65144a](https://github.com/vaam-apps/image-resizer/commit/e65144ab4235b84956827007122b1094511c904f))
* SIMD resampling via fast_image_resize ([#63](https://github.com/vaam-apps/image-resizer/issues/63) stage 1) ([1a8c7cc](https://github.com/vaam-apps/image-resizer/commit/1a8c7cca512e20625b58d347298e98a8ea42d959))
* skip EXIF extraction when strip_metadata discards it ([#88](https://github.com/vaam-apps/image-resizer/issues/88)) ([f6b0739](https://github.com/vaam-apps/image-resizer/commit/f6b0739d54f5ace37e617bcbb8f8a5c73473b5a2))
* skip EXIF extraction when stripping, and make metadata cost measurable ([#88](https://github.com/vaam-apps/image-resizer/issues/88)) ([eaedaf4](https://github.com/vaam-apps/image-resizer/commit/eaedaf4b83501340d31ab804ff94dcaaedaf54af))


### Refactoring

* extract encode_png, fix stale AVIF doc citations ([#99](https://github.com/vaam-apps/image-resizer/issues/99)) ([18edfb4](https://github.com/vaam-apps/image-resizer/commit/18edfb4fc3f4732f0ab3dc26bdc5170aa30dbaa6))


### Documentation

* **adr:** re-measure AVIF against the encoders we actually ship (0005, closes [#93](https://github.com/vaam-apps/image-resizer/issues/93)) ([c9d031b](https://github.com/vaam-apps/image-resizer/commit/c9d031b6427d084d28541dc8ceec96b9e9382f4e))
* **adr:** re-measure AVIF against the encoders we actually ship (0005) ([83980e8](https://github.com/vaam-apps/image-resizer/commit/83980e8317f75afad6705c0c4f9cfa513a074005))
* bring README and performance docs in line with current codebase ([f66dac2](https://github.com/vaam-apps/image-resizer/commit/f66dac2d175669edc547f1a30a764049b9ccd2fe))
* **ci:** the cargo-deny comment described the opposite of what runs ([b0389f6](https://github.com/vaam-apps/image-resizer/commit/b0389f63b900a9e2e003637a22bbd8762371e3d0))
* correct the keep-path claim in the previous commit ([aa0428c](https://github.com/vaam-apps/image-resizer/commit/aa0428c436dd003921bc09933dcc9dd29f0908a1))
* point the AVIF code comments at 0005, and record why quality stays 80 ([cc8eeba](https://github.com/vaam-apps/image-resizer/commit/cc8eeba05a697f297211d0794213a5f42bb29d93))
* re-audit against the code after the codec work ([#96](https://github.com/vaam-apps/image-resizer/issues/96)) ([9dd1eee](https://github.com/vaam-apps/image-resizer/commit/9dd1eee371cdc4aa0bb8d8883883af9785f6c667))
* re-point AVIF citations at adr/0005 now that 0004 is superseded ([#97](https://github.com/vaam-apps/image-resizer/issues/97)) ([88ce884](https://github.com/vaam-apps/image-resizer/commit/88ce884f0a2e50fec4d7a344cd3034a6904e8364))
* record the g:/gravity decision and its compatibility cost ([#73](https://github.com/vaam-apps/image-resizer/issues/73)) ([56f1d2e](https://github.com/vaam-apps/image-resizer/commit/56f1d2e04d1c513853bcb06542c31a0aefb0c941))
* record the post-[#67](https://github.com/vaam-apps/image-resizer/issues/67) baseline for both benchmark layers ([#82](https://github.com/vaam-apps/image-resizer/issues/82)) ([6f9bae3](https://github.com/vaam-apps/image-resizer/commit/6f9bae3e74f92564130c7c8bb90cd36be15c6827))
* refresh every document against the code ([85f5df4](https://github.com/vaam-apps/image-resizer/commit/85f5df435a42bb9c67451a1d77b8ba3c1cb1eeb2))
* refresh every document against the code ([7a0bb21](https://github.com/vaam-apps/image-resizer/commit/7a0bb2157fb5572de8690f65bc660be9a6d7aca6))
* rewrite API reference and examples to match the actual URL grammar ([af0bca8](https://github.com/vaam-apps/image-resizer/commit/af0bca866c52e61a31a302ae0dfd627aa5ea22ac))
* rewrite architecture docs to match current implementation ([b96e2f8](https://github.com/vaam-apps/image-resizer/commit/b96e2f81af55c669a63370a3bd16d4d85d37bd07))
* rewrite getting-started, deployment, development and about pages ([9404de8](https://github.com/vaam-apps/image-resizer/commit/9404de80c0924cabd34db61c34f4d2ed3514843d))
* write the missing ADR 0004 (AVIF measurement) ([#78](https://github.com/vaam-apps/image-resizer/issues/78)) ([72201e3](https://github.com/vaam-apps/image-resizer/commit/72201e3ea399f94a79e97306efe04bf2d3672d88))


### Tests

* **cache:** cover Avif and Gif in the key-uniqueness sweep ([603ba9d](https://github.com/vaam-apps/image-resizer/commit/603ba9db1c39ff936b32ba855b2bcaf53695d0f0))
* **cache:** cover Avif and Gif in the key-uniqueness sweep ([2e47158](https://github.com/vaam-apps/image-resizer/commit/2e471580a2e445f8df00bccf96e96de5899310d2))


### Build System

* Docker plumbing for libavif/AOM/dav1d, benches, docs ([#67](https://github.com/vaam-apps/image-resizer/issues/67)/[#68](https://github.com/vaam-apps/image-resizer/issues/68)) ([6db06a2](https://github.com/vaam-apps/image-resizer/commit/6db06a263d096df0c1864ec70fea27ae3e3e24a5))


### Continuous Integration

* add the helm repo before resolving chart dependencies ([f2d41e7](https://github.com/vaam-apps/image-resizer/commit/f2d41e72afa2819f51d129aacf6ae4e699e0af0b))
* adopt org-wide SAST, lint, Trivy and issue governance ([#111](https://github.com/vaam-apps/image-resizer/issues/111)) ([91e4147](https://github.com/vaam-apps/image-resizer/commit/91e4147bba5667b5f47863c95c3e8ccf1cf1d1eb))
* adopt release-please for automated version bumps and releases ([#115](https://github.com/vaam-apps/image-resizer/issues/115)) ([eeb7582](https://github.com/vaam-apps/image-resizer/commit/eeb7582f893f2857b2d8dc149090aedfd078f913))
* install the native codec build tools on the runners ([c31b3dc](https://github.com/vaam-apps/image-resizer/commit/c31b3dc07411ec58e6caa62ade5febc40491d6b4))
* re-pin org reusable workflows for the MD024 changelog fix ([358af37](https://github.com/vaam-apps/image-resizer/commit/358af378519c348b2c1531f86ab278bec6aa60aa))
* re-pin org reusable workflows for the MD024 changelog fix ([1fa34eb](https://github.com/vaam-apps/image-resizer/commit/1fa34eb526124fb34406a2f5e2a93083290735c1))
* re-pin org reusable workflows to current .github main ([#113](https://github.com/vaam-apps/image-resizer/issues/113)) ([b40b6a0](https://github.com/vaam-apps/image-resizer/commit/b40b6a0bac027edbbc9be0eda0a9896c06a17114))
* run every built image before pushing it ([#62](https://github.com/vaam-apps/image-resizer/issues/62) follow-up) ([4ea6525](https://github.com/vaam-apps/image-resizer/commit/4ea65258e0712284dbd4e6a73a1d13c27ae28273))
* run every built image before pushing it ([#62](https://github.com/vaam-apps/image-resizer/issues/62) follow-up) ([66f1e2c](https://github.com/vaam-apps/image-resizer/commit/66f1e2cd4cd1a7d16ebeb2ff8a53ce7aa4aa978a))


### Chores

* bump CACHE_KEY_VERSION to 10 for metadata stripping ([#5](https://github.com/vaam-apps/image-resizer/issues/5)) ([ad7c669](https://github.com/vaam-apps/image-resizer/commit/ad7c669f176be21a3b29fbbf7e0e043ad352f04e))
* bump CACHE_KEY_VERSION to 11 for the AVIF encoder cutover ([d8139bb](https://github.com/vaam-apps/image-resizer/commit/d8139bbce7671c5c405c151cbbaea91604c93c03))
* chart version upgrade ([b3d3009](https://github.com/vaam-apps/image-resizer/commit/b3d3009d2e49850a7b520b7a8de78ecf8e01dcc4))
* delete the temporary DSSIM harness ([#98](https://github.com/vaam-apps/image-resizer/issues/98)) ([0267ee1](https://github.com/vaam-apps/image-resizer/commit/0267ee135fa20d935cbacea8061f53622c0fbc91))
* **deps:** bump axum-otel-metrics from 0.11.0 to 0.14.1 ([#101](https://github.com/vaam-apps/image-resizer/issues/101)) ([2371abe](https://github.com/vaam-apps/image-resizer/commit/2371abe25d0e0c7606ae06d06a9bdb00b2d84514))
* **deps:** bump base64 from 0.22.1 to 0.23.1 ([#105](https://github.com/vaam-apps/image-resizer/issues/105)) ([4d152b5](https://github.com/vaam-apps/image-resizer/commit/4d152b5f69fca457bcd3faa4320da7d69ea4a1eb))
* **deps:** bump criterion from 0.5.1 to 0.8.2 ([#110](https://github.com/vaam-apps/image-resizer/issues/110)) ([09dbf54](https://github.com/vaam-apps/image-resizer/commit/09dbf54fde208894a7e04658a94de7aedcbebb15))
* **deps:** bump hmac from 0.12.1 to 0.13.0 ([#104](https://github.com/vaam-apps/image-resizer/issues/104)) ([67548bb](https://github.com/vaam-apps/image-resizer/commit/67548bbea57cf7c24d723baaf25da3d6d19540ed))
* **deps:** bump opentelemetry from 0.29.1 to 0.32.0 ([#106](https://github.com/vaam-apps/image-resizer/issues/106)) ([5d44a4b](https://github.com/vaam-apps/image-resizer/commit/5d44a4b71ad3fe4cdab6278e0f6a8db772f27af9))
* **deps:** bump opentelemetry_sdk from 0.29.0 to 0.32.1 ([#109](https://github.com/vaam-apps/image-resizer/issues/109)) ([1c3783d](https://github.com/vaam-apps/image-resizer/commit/1c3783d8f69065a51c7bfc119ca3b4e1ee9d1ead))
* **deps:** bump opentelemetry-prometheus from 0.29.1 to 0.32.0 ([#107](https://github.com/vaam-apps/image-resizer/issues/107)) ([32991ed](https://github.com/vaam-apps/image-resizer/commit/32991edb28857b137f69ffd320d7eacd7756ac37))
* **deps:** bump reqwest from 0.12.28 to 0.13.5 ([#103](https://github.com/vaam-apps/image-resizer/issues/103)) ([c818bb1](https://github.com/vaam-apps/image-resizer/commit/c818bb100111b756a7180548f6363f10f8937e0f))
* **deps:** bump super-linter/super-linter in /.github/workflows ([#61](https://github.com/vaam-apps/image-resizer/issues/61)) ([fa1599b](https://github.com/vaam-apps/image-resizer/commit/fa1599b2a5c3b322479985f57014da857d7c4024))
* enable image avif+gif features and record third-party notices ([7a9c875](https://github.com/vaam-apps/image-resizer/commit/7a9c8755708891706f195e7b628c4ddfd7ad2e6c))
* FUNDING.yml ([a165476](https://github.com/vaam-apps/image-resizer/commit/a165476a2c192c0c9cc60108326a2e6d6174e010))
* miaou ([315c19a](https://github.com/vaam-apps/image-resizer/commit/315c19a8593d14d7d3516d76ddf7efd16e513583))
