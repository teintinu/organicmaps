# Build
FROM rust:1-slim-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked

# Run
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 hazardd
COPY --from=build /app/target/release/hazardd /usr/local/bin/hazardd
USER hazardd
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["hazardd"]
