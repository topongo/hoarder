FROM rust:1 AS builder

WORKDIR /src
RUN rustup target add x86_64-unknown-linux-musl
# prepare dependencies cache
RUN mkdir -p src
RUN echo "fn main() {}" > src/main.rs
COPY Cargo.toml Cargo.lock .
RUN cargo build --release --target x86_64-unknown-linux-musl
# actual build
COPY src src
RUN touch src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM topongo/alpine-docker-restic AS runner

COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/hoarder /usr/bin/hoarder

