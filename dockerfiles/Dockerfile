FROM ubuntu:24.04 AS builder

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl build-essential cmake git python3-pip \
      libssl-dev pkg-config ca-certificates zip \
      libudev-dev \
    && apt clean && \
    useradd -m esp &&\
    mkdir -p /app && \
    chown -R esp:esp /app && \
    mkdir -p  0777 /usr/local/rustup && \
    mkdir -p  0777 /usr/local/cargo && \
    mkdir -p  0777 /usr/local/cargo/bin && \
    chown -R esp:esp /usr/local/rustup && \
    chown -R esp:esp /usr/local/cargo && \
    chown -R esp:esp /usr/local/cargo/bin

USER esp

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path && \
    cargo install espup espflash  && \
    espup install  && \
    . $HOME/export-esp.sh && \
    rustup default esp

WORKDIR /app

COPY --chmod=755 ./dockerfiles/entrypoint.sh /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

CMD ["bash"]