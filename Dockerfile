FROM rust@sha256:251cec8da4689d180f124ef00024c2f83f79d9bf984e43c180a598119e326b84 AS backend_builder
RUN mkdir -p /build/src
WORKDIR /build
COPY Cargo.lock Cargo.toml /build/
RUN echo "fn main(){}" > /build/src/main.rs
RUN cargo build --release
COPY src /build/src
RUN touch /build/src/main.rs
RUN cargo build --release
RUN cp /build/target/release/analytics-collector /build/analytics-collector

FROM debian@sha256:bd73076dc2cd9c88f48b5b358328f24f2a4289811bd73787c031e20db9f97123
RUN mkdir -p /opt/analytics-collector
WORKDIR /opt/analytics-collector
COPY --from=backend_builder /build/analytics-collector /opt/analytics-collector/
USER 1000
ENTRYPOINT ["/opt/analytics-collector/analytics-collector"]
