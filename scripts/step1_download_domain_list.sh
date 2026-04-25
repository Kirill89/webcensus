#!/bin/bash

wget \
  "https://raw.githubusercontent.com/zakird/crux-top-lists/refs/heads/main/data/global/current.csv.gz" \
  -O /root/data/crux.gz

gunzip -c /root/data/crux.gz | tail -n +2 | cut -d, -f1 | sed -E 's#^https?://##' > /root/data/domains.txt
