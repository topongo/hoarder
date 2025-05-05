FROM rust:1 AS builder

WORKDIR /src
# prepare dependencies cache
RUN mkdir -p src
RUN echo "fn main() {}" > src/main.rs
COPY Cargo.toml Cargo.lock .
RUN cargo build --release
# actual build
COPY src src
RUN touch src/main.rs
RUN cargo build --release

FROM debian:bookworm-slim AS runner

RUN apt-get update
RUN apt-get install --yes \
      docker \
      docker-compose \
      restic

COPY --from=builder /src/target/release/hoarder /usr/local/bin/hoarder

