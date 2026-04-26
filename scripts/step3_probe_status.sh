#!/bin/bash
set -euo pipefail

PATH_TO_PROBE="${1:-/.well-known/security.txt}"

./skim/target/release/skim --input data/dns.ndjson --output data/status.ndjson --path "$PATH_TO_PROBE" --concurrency 32 --start-line 1
