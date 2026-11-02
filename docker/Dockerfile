# chcsv-rust: IC(Instant Client)同梱イメージを作る Dockerfile
#
# ビルドは rockylinux:8 コンテナ（glibc 2.28 下限）で、CI/release-build と同じ手順。
# ランタイムは Oracle Linux 8 + Instant Client basic-lite。
# IC は Oracle 無償ライセンスの ZIP をこの場で取得する（認証不要 URL）。
#   Instant Client は OTN Development and Distribution License Terms の下、
#   自社アプリと束ねての無償再配布が許可されている（OTN条項の再配布先への
#   提示が条件）。本リポジトリに IC バイナリは含めず、ビルド時に Oracle
#   公式 URLs から取得する。ビルド済みイメージを公開する場合はこの条項
#   参照: https://www.oracle.com/downloads/licenses/instant-client-lic.html
#
# 使い方: docker compose run --rm -T chcsv user/pass@oracle:1521/XEPDB1 -h < query.sql

FROM rockylinux:8 AS builder
RUN dnf -y install git gcc make curl tar gzip which unzip && dnf clean all
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
WORKDIR /src
COPY . .
RUN ./setup.sh && cargo build --release \
    && strip target/release/chcsv

FROM rockylinux:8 AS ic
ARG IC_DIR=2124000
ARG IC_VER=21.24.0.0.0dbru
RUN dnf -y install curl unzip && \
    curl -fsSLo /tmp/ic.zip "https://download.oracle.com/otn_software/linux/instantclient/${IC_DIR}/instantclient-basiclite-linux.x64-${IC_VER}.zip" && \
    unzip -oq /tmp/ic.zip -d /opt && \
    mv /opt/instantclient_* /opt/instantclient && \
    rm /tmp/ic.zip

FROM oraclelinux:8
RUN dnf -y install libaio libnsl && dnf clean all
COPY --from=ic /opt/instantclient /opt/instantclient
ENV LD_LIBRARY_PATH=/opt/instantclient
COPY --from=builder /src/target/release/chcsv /usr/local/bin/chcsv
ENTRYPOINT ["/usr/local/bin/chcsv"]
