# syntax=docker/dockerfile:1

# ---- build stage ----
FROM rust:1-bookworm AS build
WORKDIR /src
# Manifests + sources (probe is a workspace member, so it must be present).
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY probe ./probe
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --bin dwg2png \
 && cp target/release/dwg2png /usr/local/bin/dwg2png \
 && strip /usr/local/bin/dwg2png

# ---- runtime stage ----
FROM debian:bookworm-slim
# DejaVu Sans covers Latin + Cyrillic glyphs used in CAD labels.
RUN apt-get update \
 && apt-get install -y --no-install-recommends fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*
ENV DWG2PNG_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
COPY --from=build /usr/local/bin/dwg2png /usr/local/bin/dwg2png
WORKDIR /work
ENTRYPOINT ["dwg2png"]
CMD ["--help"]
