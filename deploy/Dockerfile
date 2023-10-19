FROM rust

WORKDIR /opt/hitscanner

# python
RUN apt-get update && \
    apt-get install -y python3-pip

# scamper
COPY scamper-cvs-20230224.tar.gz ./
RUN tar zxvf scamper-cvs-20230224.tar.gz
WORKDIR scamper-cvs-20230224
RUN sed -i -e '199 a\\tstring_concat(header, sizeof(header), &off, " %ld",\n\t\ttrace->start.tv_sec);' scamper/trace/scamper_trace_text.c
RUN ./configure \
    && make \
    && make install
RUN echo 'include /usr/local/lib' >> /etc/ld.so.conf && ldconfig
WORKDIR /opt/hitscanner

# iffinder
COPY iffinder-1.38.tar.gz ./
RUN tar zxvf iffinder-1.38.tar.gz
WORKDIR iffinder-1.38
RUN ./configure \
    && make \
    && ln -s $(realpath miniffinder) /usr/bin/iffinder
WORKDIR /opt/hitscanner

# hitscanner
COPY hitscanner ./hitscanner
WORKDIR hitscanner
RUN cargo build --release \
    && cargo install --path .
WORKDIR /opt/hitscanner

CMD /bin/sh