FROM golang:1.21-bookworm AS builder

WORKDIR /f3

COPY * /f3

RUN go build . && strip f3-sidecar

FROM ubuntu:noble

WORKDIR /f3

COPY --from=builder /f3/f3-sidecar /usr/local/bin/

RUN f3-sidecar -h

CMD ["f3-sidecar"]
