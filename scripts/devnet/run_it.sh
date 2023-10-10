#!/bin/bash
docker-compose down --rmi all --volumes
docker build -t lotus-devnet -f lotus.dockerfile .
docker-compose up --build --force-recreate

