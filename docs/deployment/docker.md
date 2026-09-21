# Docker Deployment

This guide explains how to build and run `emgr` with Docker.

## Prerequisites

- Docker, with Buildx (for `docker build`).
- Docker Compose v2 (`docker compose`, not the standalone `docker-compose`)
  if you use `compose.yaml`.

## Building the Docker image

The `Dockerfile` in the project root defines **four deploy targets** and no
default/unnamed final stage, so `--target` is required:

| Target           | Storage          | OpenTelemetry / `/metrics` |
| ---------------- | ---------------- | -------------------------- |
| `fs_deploy`      | local filesystem | no                         |
| `fs_otel_deploy` | local filesystem | yes                        |
| `s3_deploy`      | S3 / MinIO       | no                         |
| `s3_otel_deploy` | S3 / MinIO       | yes                        |

`healthcheck` (the binary the image's own `HEALTHCHECK` runs) is built into
all four automatically.

```bash
cd /path/to/image-resizer

# Build the image (local filesystem storage, no OTel)
docker build --target fs_deploy -t emgr:latest .
```

### The builder/runtime base image pin - do not bump casually

The builder stage is pinned by digest to a **Debian 12 ("bookworm")** Rust
image, and the runtime stage to `gcr.io/distroless/cc-debian12` - on
purpose, and they must move together. A previous version of this Dockerfile
used a Debian 13 ("trixie") builder (glibc 2.41) against this same
bookworm-based runtime (glibc 2.36); the `s3` build's native C dependency at
the time (`aws-lc-sys`) referenced `GLIBC_2.38` symbols, and the resulting
image built cleanly, pushed successfully, and then died instantly on start
with:

```text
version 'GLIBC_2.38' not found
```

CI never caught it, because the build pipeline built and pushed images
without ever running one - see the "run every built image before pushing
it" fix (`.github/workflows/build.yml`) that closed that gap. `local_fs`
survived only by luck (it happens not to touch the symbols in question).

`aws-lc-sys` is gone (#134 replaced it with the pure-Rust `rustls-graviola`
crypto provider), so that specific trigger no longer exists - nothing left
in the dependency graph compiles C, and nothing reaches for a glibc symbol
the way `aws-lc-sys` did. The rule itself still stands regardless: it was
never really about `aws-lc-sys` specifically, the Rust `std` binary links
the builder's glibc either way. If you ever need to bump either base image,
bump both in lockstep and actually run the resulting image before merging -
don't rely on CI's static checks to catch a glibc mismatch, they won't.

### No native build tools - `emgr` has no C or C++ dependencies

Every image codec this service links against - JPEG (`jpeg-decoder`/
`jpeg-encoder`), WebP (`vaam-image-webp`), and AVIF encode/decode
(`ravif`/`avif-decode`) - is pure Rust, built as part of the normal `cargo
build` with no C compiler, assembler, or meta build system involved. This
used to be the opposite: `mozjpeg` (JPEG), the `webp` crate (WebP,
libwebp), and `libavif-sys` (AVIF, vendoring libavif + AOM + dav1d) each
built vendored C source through their own `*-sys` crates' `build.rs`,
needing `nasm` (libjpeg-turbo's x86_64 SIMD path), `cmake`
(`libavif-sys`/`libaom-sys`), and `meson`+`ninja-build` (`libdav1d-sys`)
installed in the builder stage. #134 removed the apt layer that installed
those tools entirely - there's nothing to install any more, on any
platform this Dockerfile targets.

A `no-native-deps` CI job now guards this: it fails the build if any
dependency reintroduces a native (`-sys`) library, so a future dependency
bump that silently pulls C back in gets caught in CI instead of surfacing
as a missing build tool in this Dockerfile. The runtime image's own shape
(base, size, contents) was already unaffected by which codec libraries the
builder linked - none of the four `deploy` stages, the `base_deploy` they
build on, or the runtime `HEALTHCHECK`/`ENTRYPOINT` setup needed any
change for #63/#66/#67/#68 (when the codecs were still native) or for #134
(now that they aren't) - only the builder stage's `apt-get install` line
(now removed entirely) and the Rust dependency graph did.

## Running the container

`emgr` fails closed at startup - see
[Installation](../getting-started/installation.md#configure-the-two-startup-checks)
for the full explanation. In short, every run needs:

- `SIGNING_KEY` + `SIGNING_SALT` (hex-encoded), or `ALLOW_UNSIGNED_REQUESTS=true`.
- On an `*_otel_deploy` image only: `METRICS_AUTH_TOKEN`, or
  `ALLOW_UNAUTHENTICATED_METRICS=true`.

A container started without satisfying these exits immediately with a
clear error - it does not hang or serve broken responses.

### Basic run (local filesystem, signing disabled for local testing)

The service listens on port `3000` by default (`PORT`, `HOST`):

```bash
docker run -d -p 3000:3000 \
  -e ALLOW_UNSIGNED_REQUESTS=true \
  -e LOCAL_FS_STORAGE_PATH=/app/data/images \
  --name emgr-app \
  emgr:latest
```

### With S3/MinIO storage and real signing

You can configure the service using environment variables - see
[Configuration](../getting-started/configuration.md) for the full list.

```bash
docker run -d -p 3000:3000 \
  -e SIGNING_KEY=$(openssl rand -hex 32) \
  -e SIGNING_SALT=$(openssl rand -hex 16) \
  -e STORAGE_TYPE=S3 \
  -e MINIO_ENDPOINT_URL=https://s3.amazonaws.com \
  -e MINIO_BUCKET=my-image-bucket \
  -e MINIO_ACCESS_KEY_ID=YOUR_ACCESS_KEY \
  -e MINIO_SECRET_ACCESS_KEY=YOUR_SECRET_KEY \
  -e MINIO_REGION=us-east-1 \
  --name emgr-app \
  ghcr.io/vaam-store/image-resizer:s3-latest
```

`ghcr.io/vaam-store/image-resizer:s3-latest` (built from the `s3_deploy`
target) is what `.github/workflows/build.yml` publishes on every push to
`main`, alongside `fs-latest`, `fs_otel-latest` and `s3_otel-latest` (each
also gets a per-commit `<flavor>-<sha>` tag). There is no floating,
un-prefixed `latest` tag - build.yml has no release-tag trigger today, so
none of these are semver-stable either; pin a specific `<flavor>-<sha>`
for anything beyond local testing. Only build locally with
`docker build --target s3_deploy` if you need an unpublished change.

### Using Docker Compose

`compose.yaml` in the project root brings up the `app` service (local
filesystem, `fs_otel_deploy` target) and `app-s3` (`s3_otel_deploy`
target, backed by a local MinIO), plus a Jaeger all-in-one container for
the OTel traces both services emit - both are `*_otel_deploy` builds, so
both need `/metrics` auth configured, not just signing.

`compose.yaml`'s `environment:` blocks for `app` and `app-s3` forward
`SIGNING_KEY`/`SIGNING_SALT`/`METRICS_AUTH_TOKEN` from your shell/`.env`
(empty by default), and default `ALLOW_UNSIGNED_REQUESTS` and
`ALLOW_UNAUTHENTICATED_METRICS` to `true` (GH #84). `compose.yaml` exists
for local development, so that default is deliberate - you can
`docker compose up` with no `.env` at all and get a running service,
without generating an HMAC key first. If you set real
`SIGNING_KEY`/`SIGNING_SALT`/`METRICS_AUTH_TOKEN` values in `.env`, they
take effect regardless of the `ALLOW_*` defaults - a real key/salt is
checked independently of `ALLOW_UNSIGNED_REQUESTS`
(`src/modules/signing/config.rs`), so the two never conflict.

```bash
# Start everything (no .env needed - signing/metrics auth default to
# disabled for local development)
docker compose up -d --build

# Or, to exercise real signing/metrics auth locally:
cp .env.example .env   # uncomment/set SIGNING_KEY, SIGNING_SALT, METRICS_AUTH_TOKEN in it
docker compose up -d --build

# Stop
docker compose down

# View logs
docker compose logs -f
```

The `Makefile` wraps the same compose invocations (`make up`, `make down`,
`make logs`, `make ps` - see `make help` for the full list) with the
project name pinned to `emgr`.

## Managing the container

- **View logs**: `docker logs emgr-app`
- **Stop the container**: `docker stop emgr-app`
- **Start the container**: `docker start emgr-app`
- **Remove the container**: `docker rm emgr-app`

## Pushing to a Docker registry

If you want to deploy a locally built image to a remote environment (like
Kubernetes), push it to a registry (Docker Hub, AWS ECR, Google GCR, ...):

```bash
# Tag the image (replace <your-registry-username> and <repository-name>)
docker tag emgr:latest <your-registry-username>/<repository-name>:latest

# Log in to your Docker registry
docker login

# Push the image
docker push <your-registry-username>/<repository-name>:latest
```

In this repository, `.github/workflows/build.yml` does this automatically
on every push, publishing all four flavours to
`ghcr.io/vaam-store/image-resizer` - see the tag naming note above.
