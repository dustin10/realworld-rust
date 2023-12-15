#!/bin/sh

docker exec broker kafka-topics --bootstrap-server broker:9092 --create --topic article --partitions 2 --replication-factor 1
docker exec broker kafka-topics --bootstrap-server broker:9092 --create --topic user --partitions 2 --replication-factor 1
