#!/bin/bash

massdns \
  --resolvers resolvers.txt \
  --type A \
  --output J \
  --retry REFUSED \
  --retry SERVFAIL \
  "/root/data/domains.txt" > "/root/data/dns.ndjson"
