#!/bin/bash
set -euo pipefail

mkdir -p data/unfiltered
cd data/unfiltered

curl --parallel \
  --parallel-max 50 \
  --parallel-immediate \
  --connect-timeout 3 \
  --max-time 10 \
  --max-filesize 5M \
  --retry 2 \
  --retry-delay 5 \
  --retry-max-time 30 \
  --user-agent "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36" \
  -K ../urls.curl
