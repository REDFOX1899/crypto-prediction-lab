FROM rust:1-slim-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
RUN cargo build --locked --release -p dashboard

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/dashboard /usr/local/bin/dashboard
ENV DASHBOARD_BIND_ADDRESS=0.0.0.0
USER 65532:65532
EXPOSE 8080
CMD ["dashboard"]
