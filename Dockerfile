# Small non-root container for the BicMath MCP server (stdio) or HTTP transport.
FROM rust:1.96-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p bicmath --features bicmath/http

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --create-home bicmath
COPY --from=builder /build/target/release/bicmath /usr/local/bin/bicmath
USER bicmath
WORKDIR /home/bicmath
# stdio MCP by default; override with:
#   docker run --rm -i bicmath serve --transport stdio
#   docker run --rm -p 8080:8080 bicmath serve --transport http --bind 0.0.0.0:8080
ENTRYPOINT ["bicmath"]
CMD ["serve", "--transport", "stdio"]
