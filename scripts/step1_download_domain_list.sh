#!/bin/bash
set -euo pipefail

wget \
  "https://raw.githubusercontent.com/zakird/crux-top-lists/refs/heads/main/data/global/current.csv.gz" \
  -O data/crux.gz

gunzip -c data/crux.gz | tail -n +2 | cut -d, -f1 | sed -E 's#^https?://##' > data/domains.txt
