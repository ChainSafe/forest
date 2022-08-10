FROM ubuntu:22.10

ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update
RUN apt-get install -y s3fs ca-certificates curl gnupg lsb-release docker-compose

COPY docker-compose.yml .
COPY run_service.sh .
RUN chmod +x run_service.sh
ENTRYPOINT /run_service.sh

