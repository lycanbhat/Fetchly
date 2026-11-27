# Fetchly web: builds the Dioxus web app + the proxy server into one small image.
#   docker build -t fetchly .
#   docker run -p 8080:8080 -e FETCHLY_PROXY_TOKEN=change-me fetchly
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown \
 && rustup component add llvm-tools \
 && cargo install dioxus-cli@0.7.10 --locked
WORKDIR /src
COPY . .
RUN dx build --platform web --release -p fetchly \
 && cargo build --release -p fetchly-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/fetchly-server /usr/local/bin/fetchly-server
COPY --from=build /src/target/dx/fetchly/release/web/public /srv/fetchly
EXPOSE 8080
CMD ["fetchly-server", "--host", "0.0.0.0", "--port", "8080", "--dist", "/srv/fetchly"]
