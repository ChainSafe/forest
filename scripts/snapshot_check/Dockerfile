FROM ubuntu:22.10

# Install dependencies
RUN apt-get update
RUN apt-get install --no-install-recommends -y s3fs git ca-certificates docker-compose

COPY run_service.sh /root/run_service.sh
RUN chmod +x /root/run_service.sh
ENTRYPOINT /root/run_service.sh

