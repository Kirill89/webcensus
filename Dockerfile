FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash ca-certificates curl git jq openssh-client sudo \
    build-essential python3 python3-pip python3-venv unzip \
    libpcap-dev libpcap0.8 wget

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
RUN apt-get install -y --no-install-recommends nodejs
RUN rm -rf /var/lib/apt/lists/*
RUN npm install -g bun@1.3.13

ENV HOME=/root \
    USER=root \
    SHELL=/bin/bash \
    NPM_CONFIG_PREFIX=/root/.npm-global

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal

ENV PATH=/root/.cargo/bin:/root/.npm-global/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN git init /massdns && \
    cd /massdns && \
    git remote add origin https://github.com/blechschmidt/massdns && \
    git fetch --depth=1 origin 6bfa47197d78e68b79041d494e280174cb2d6ae1 && \
    git checkout FETCH_HEAD && \
    make && \
    make install

WORKDIR /root

COPY resolvers.txt /root/

RUN sed -i -E 's/[[:space:]]*\/\/.*$//; s/[[:space:]]+$//' resolvers.txt

COPY skim /root/skim

RUN cd skim && cargo build --release

COPY scripts /root/scripts
