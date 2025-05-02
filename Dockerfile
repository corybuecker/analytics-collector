FROM rust@sha256:640960fe15de2f67cc88db7f0f547977cb759cba9eab246df29d98d02aaf24b8 AS backend_builder
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

FROM debian@sha256:00cd074b40c4d99ff0c24540bdde0533ca3791edcdac0de36d6b9fb3260d89e2
RUN mkdir -p /opt/analytics-collector
WORKDIR /opt/analytics-collector
COPY --from=backend_builder /build/analytics-collector /opt/analytics-collector/
USER 1000
ENTRYPOINT ["/opt/analytics-collector/analytics-collector"]
