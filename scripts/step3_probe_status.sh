#!/bin/bash

/root/skim/target/release/skim --input /root/data/dns.ndjson --output /root/data/status.ndjson --path "/.well-known/security.txt" --concurrency 100 --start-line 0
