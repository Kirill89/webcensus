#!/bin/bash
set -euo pipefail

massdns \
  --resolvers resolvers.txt \
  --type A \
  --output J \
  --retry REFUSED \
  --retry SERVFAIL \
  "data/domains.txt" > "data/dns.ndjson"
