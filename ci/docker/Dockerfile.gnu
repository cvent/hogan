FROM rustembedded/cross:x86_64-unknown-linux-gnu

COPY openssl.sh /
RUN bash /openssl.sh linux-x86_64 x86_64-linux-gnu-

RUN apt install -y llvm-dev libclang-dev clang

ENV OPENSSL_ROOT_DIR=/openssl \
    LCB_NO_PLUGINS=1

# COPY libcouchbase.sh /
# RUN bash /libcouchbase.sh

