# ponytail: CI-parity Linux build check (no display server)
FROM rust:1-bookworm

RUN apt-get update -qq \
    && apt-get install -y --no-install-recommends \
        libvulkan-dev \
        libxkbcommon-dev \
        libwayland-dev \
        libx11-dev \
        libxrandr-dev \
        libxi-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo fmt --all --check \
    && cargo clippy --workspace --all-targets --locked -- -D warnings \
    && cargo test --workspace --all-targets --locked \
    && cargo build --locked --release -p drift-wallpaper