# syntax=docker/dockerfile:1
#
# Lightweight, fully-static ctos image.
#
#   Stage 1 (builder): compile a static musl binary on Alpine. The vendored
#                      builtin tokenizers committed in the repo are embedded via
#                      include_bytes!, so the binary is self-contained & offline.
#   Stage 2 (final):   `scratch` — nothing but the binary. Tiny, no shell, no OS.
#
# Build:  docker build -t ctos .
# Use:    docker run --rm -v "$PWD:/work" ctos /work            # count CWD
#         docker run --rm -v "$PWD:/work" ctos check /work/skills
#         docker run --rm ctos --version

FROM rust:alpine AS builder

# musl-dev + build-base: C toolchain for the onig (oniguruma) dependency of the
# `tokenizers` crate; everything links statically against musl.
RUN apk add --no-cache musl-dev build-base

WORKDIR /src

# Leverage layer caching: fetch deps against the manifests first.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
 && cargo build --release 2>/dev/null || true
RUN rm -rf src

# Now copy the real sources (incl. build.rs, config/, and vendored tokenizers/).
COPY . .

# Ensure the build script re-runs against the real tree, then build.
RUN touch build.rs && cargo build --release \
 && strip target/release/ctos

# Marker so `ctos update` can refuse inside the image (update = docker pull).
RUN touch /ctos-docker-marker

########################################
FROM scratch AS final

LABEL org.opencontainers.image.title="ctos" \
      org.opencontainers.image.description="count tokens of skill — cloc-style token counter" \
      org.opencontainers.image.source="https://github.com/leftvalue/ctos" \
      org.opencontainers.image.licenses="GPL-3.0"

COPY --from=builder /src/target/release/ctos /ctos
COPY --from=builder /ctos-docker-marker /.ctos-docker

# Mount your project at /work and pass paths under it.
WORKDIR /work

ENTRYPOINT ["/ctos"]
CMD ["--help"]
