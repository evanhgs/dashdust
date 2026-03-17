FROM --platform=$BUILDPLATFORM alpine:3.21 AS trunk-downloader

RUN apk add --no-cache curl tar

RUN TRUNK_VERSION="0.21.14" && \
    curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xzf - -C /usr/local/bin trunk && \
    chmod +x /usr/local/bin/trunk

FROM --platform=$BUILDPLATFORM rust:alpine AS frontend-builder

RUN apk add --no-cache musl-dev

COPY --from=trunk-downloader /usr/local/bin/trunk /usr/local/bin/trunk

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY Cargo.toml ./
COPY backend/Cargo.toml ./backend/Cargo.toml
COPY frontend/Cargo.toml ./frontend/Cargo.toml
COPY frontend/index.html ./frontend/index.html

RUN mkdir -p backend/src frontend/src && \
    echo 'fn main(){}' > backend/src/main.rs && \
    echo 'fn main(){}' > frontend/src/main.rs

RUN cd frontend && trunk build --release || true

COPY backend ./backend
COPY frontend/src ./frontend/src
COPY frontend/favicon.ico ./frontend/favicon.ico

WORKDIR /app/frontend
RUN trunk build --release

FROM rust:alpine AS backend-builder

RUN apk add --no-cache musl-dev

WORKDIR /app
COPY Cargo.toml ./
COPY backend/Cargo.toml ./backend/Cargo.toml
COPY frontend/Cargo.toml ./frontend/Cargo.toml

RUN mkdir -p backend/src frontend/src && \
    echo 'fn main(){}' > backend/src/main.rs && \
    echo 'fn main(){}' > frontend/src/main.rs

RUN cargo build --release --package backend 2>/dev/null; true

COPY backend/src ./backend/src
RUN touch backend/src/main.rs && \
    cargo build --release --package backend

FROM alpine:3.21

RUN apk add --no-cache ca-certificates

COPY --from=backend-builder /app/target/release/backend /backend
COPY --from=frontend-builder /app/frontend/dist          /dist

WORKDIR /

EXPOSE 3000

ENTRYPOINT ["/backend"]
