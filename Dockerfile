FROM rust@sha256:7b65306dd21304f48c22be08d6a3e41001eef738b3bd3a5da51119c802321883 AS backend_builder
RUN mkdir -p /build/src
WORKDIR /build
COPY Cargo.lock Cargo.toml /build/
RUN echo "fn main(){}" > /build/src/main.rs
RUN cargo build --release
COPY src /build/src
RUN touch /build/src/main.rs
COPY templates /build/templates
RUN cargo build --release
RUN cp /build/target/release/analytics-collector /build/analytics-collector

FROM debian@sha256:264982ff4d18000fa74540837e2c43ca5137a53a83f8f62c7b3803c0f0bdcd56
RUN mkdir -p /opt/analytics-collector
WORKDIR /opt/analytics-collector
COPY --from=backend_builder /build/analytics-collector /opt/analytics-collector/
USER 1000
ENTRYPOINT ["/opt/analytics-collector/analytics-collector"]
