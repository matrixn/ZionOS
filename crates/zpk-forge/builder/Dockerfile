FROM alpine:3.23@sha256:ff7f0c727dd796aa36787bfcd9a64bf35a6ce72a73451b3359fde849b6703d63
RUN apk add --no-cache rust cargo gcc g++ musl-dev cmake make go binutils patchelf python3 \
    && mkdir -p /src && chmod 755 /src
# The CLI supplies only this trusted directory as context, never the application tree.
COPY driver.py /usr/local/lib/forge-driver.py
RUN chmod 555 /usr/local/lib/forge-driver.py
USER 10000:10000
ENTRYPOINT ["/bin/sleep", "900"]
