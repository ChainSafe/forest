FROM ubuntu:22.10

ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update
RUN apt-get install -y s3fs ca-certificates curl gnupg lsb-release docker-compose
#RUN mkdir -p /etc/apt/keyrings
#RUN curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
#RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null

#RUN apt-get update
#RUN apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin

COPY docker-compose.yml .
COPY run_service.sh .
COPY sync_check.sh .
COPY health_check.sh .
RUN chmod +x run_service.sh sync_check.sh health_check.sh
ENTRYPOINT /run_service.sh

