#!/bin/bash
set -euo pipefail

./skim/target/release/skim --input data/dns.ndjson --output data/status.ndjson --path "/.well-known/security.txt" --concurrency 100 --start-line 1
