#!/bin/sh

: ${ZOOKEEPER:=localhost:2181}

kafka-topics.sh \
    --create \
    --zookeeper "$ZOOKEEPER" \
    --topic feeds \
    --partitions 1 \
    --replication-factor 1 \
    --config cleanup.policy=compact \
    --config compression.type=uncompressed \
    --config max.message.bytes=$((4 * 1024))
