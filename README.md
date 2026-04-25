# webcensus

A fast pipeline for hunting a specific file path (e.g. `/.well-known/security.txt`,
`/robots.txt`, `/ads.txt`, `/humans.txt`, `/sitemap.xml`) across **millions of
domains** in a reasonable amount of time on a single machine.

> ⚠️ **Disclaimer.** This project sends DNS and HTTP traffic to large numbers
> of third-party hosts. You are solely responsible for how you use it,
> including compliance with applicable laws, terms of service, acceptable-use
> policies, and the rules of any networks involved. The authors provide this
> software **as-is**, with no warranties of any kind, and accept no liability
> for any damage, abuse complaints, blocked IPs, legal trouble, or other
> consequences arising from its use. **All risk is on the operator.** See
> [LICENSE](LICENSE).

The whole thing runs inside a reproducible Docker sandbox. Each stage is a
narrow funnel that throws away non-candidates as cheaply as possible, so the
expensive stages only see hosts that survived the previous filter.

```
domain list  ──►  DNS (A records)  ──►  HTTPS probe  ──►  download & verify
  ~10M             massdns              skim (Rust)        bun script
  domains          ~100k qps            status + cert      content-shape gate
```

## How to use

```sh
make shell                                # build & enter the sandbox
./scripts/step1_download_domain_list.sh   # or bring your own domains.txt
./scripts/step2_massdns.sh
./scripts/step3_probe_status.sh
./scripts/step4_collect.sh
```

All artifacts land in `./data/` on the host (mounted into the container).

## The pipeline

### 1. Domain list — `step1_download_domain_list.sh`

Downloads the [Chrome UX Report top sites](https://github.com/zakird/crux-top-lists)
(the most "real" of the public top-N lists, ranked by actual Chrome user
visits), strips the rank column and `https://` scheme, and writes
`data/domains.txt` — one apex per line.

Swap in any other source (see [Domain list sources](#domain-list-sources)) as
long as the output is one domain per line.

### 2. DNS resolution — `step2_massdns.sh`

Runs [massdns](https://github.com/blechschmidt/massdns) against
`resolvers.txt` (a curated set of public recursive resolvers — Cloudflare,
Quad9, Google, AdGuard, OpenDNS, etc.) to fetch A records for every domain.

- Output: `data/dns.ndjson` (massdns JSON format)
- Why it's fast: massdns sends UDP queries in parallel across the resolver
  pool; tens of thousands of qps on a single box. The JSON output is the
  format `skim` consumes directly — no intermediate transformation.

### 3. HTTPS status probe — `step3_probe_status.sh`

Runs `skim`, a purpose-built async Rust prober (see `skim/`), against every
resolved host. For each host it:

1. Opens a TCP connection to `:443` (configurable).
2. Performs a TLS handshake using a **recording verifier** — chain validation
   runs to completion and the verdict is captured, but a bad cert does not
   abort the handshake. This is what lets a single pass capture both the HTTP
   status *and* whether the cert is trustworthy.
3. Sends a raw HTTP/1.1 `GET <path>` request.
4. Reads **just the status line** and closes the socket.

Output rows (`data/status.ndjson`) look like:

```json
{"url":"https://example.com/.well-known/security.txt","status":"success","code":200,"cert_ok":true}
```

Why it's fast:
- Status-line-only — no body read, no body bandwidth.
- Bounded concurrency via a `tokio` semaphore (default 100, tune with `--concurrency`).
- Tight per-stage timeouts (connect / handshake / read) so dead hosts fail in seconds.
- Resumable: pass `--start-line N` to pick up after a crash.
- Pre-pass scans the input once to count probeable rows for a real ETA;
  `--skip-precount` skips it on resume.

### 4. Download & verify — `step4_collect.sh`

Filters `status.ndjson` to rows where `code == 200 && cert_ok == true && status == "success"`,
fetches each URL with `bun`, and runs a **content-shape verifier** before
saving (`scripts/download-and-verify.mjs`).

The default `text-file` verifier rejects:
- bodies shorter than 16 chars (empty / "Not Found" / "OK"),
- bodies containing control characters (binary noise),
- bodies that look like HTML, PHP source, or JSON (servers that 200-OK every
  path with a SPA shell or a generic JSON error).

A `json` verifier mode is also available for paths that should be JSON
(`/.well-known/security.txt` is plain text, so the default is what you want
for it).

Surviving files are written to `data/files/` named after the URL — these are
your real, content-verified hits.

## Why this is fast end-to-end

The pipeline is funnel-shaped. Each step is roughly an order of magnitude
cheaper *per host* than the next, so the expensive stages only ever see a
small fraction of the input:

| Stage    | Per-host cost           | What it eliminates                       |
|----------|-------------------------|------------------------------------------|
| massdns  | one UDP round-trip      | parked / dead / NXDOMAIN domains         |
| skim     | one TLS + status line   | hosts without 443, bad cert, non-200     |
| collect  | one full HTTPS GET      | 200-OK-but-not-actually-the-file noise   |

Plus: massdns output → skim input is the same NDJSON; skim output → collect
input is the same NDJSON. No format conversions between stages.

## Domain list sources

### Top ~1M lists (free)

- **Tranco** — research-grade combined ranking. Stable, monthly. https://tranco-list.eu
- **Cisco Umbrella Top 1M** — DNS query volume from Umbrella resolver. Daily; high churn. http://s3-us-west-1.amazonaws.com/umbrella-static/index.html
- **Majestic Million** — ranked by referring subnets (link graph). Daily. https://majestic.com/reports/majestic-million
- **Cloudflare Radar Top 1M** — query volume from 1.1.1.1. https://radar.cloudflare.com/domains
- **Chrome UX Report (CrUX)** — top sites by real Chrome user visits. Most "real" of all of these. Monthly via BigQuery. https://github.com/zakird/crux-top-lists *(default in `step1`)*

### Bigger (10M+)

- **Open PageRank Top 10M** — Common Crawl link graph. https://www.domcop.com/openpagerank/what-is-openpagerank
- **DomCop Top 10M** — aggregates Open PageRank; paid for full, free sample.

### Truly massive (100M+)

- **DNS zone files** — complete list of registered domains per TLD.
    - Verisign for `.com`/`.net` (~160M `.com` alone).
    - ICANN CZDS for hundreds of TLDs: https://czds.icann.org
    - Free but requires signing an access agreement.
- **Common Crawl URL index** — extract unique domains from billions of crawled URLs. https://commoncrawl.org

### Live / continuously updated

- **Certificate Transparency logs** (crt.sh, certstream) — live feed of every TLS cert issued. Useful angle: newly-created sites are often misconfigured. https://certstream.calidog.io
- **SecurityTrails / DNSlytics** — commercial DNS aggregators with free tiers.

## Repo layout

```
Dockerfile, Makefile      — sandbox image (Ubuntu + node + bun + rust + massdns)
ai-sandbox/               — same sandbox + claude-code, for AI-assisted iteration
scripts/                  — the four pipeline steps
skim/                     — Rust HTTPS prober (status-line + cert verdict)
resolvers.txt             — curated public DNS resolvers for massdns
data/                     — pipeline outputs (gitignored, mounted from host)
```

## Citation

If you use this software in academic or research work, please cite this
repository. A BibTeX entry:

```bibtex
@software{webcensus,
  title  = {webcensus: a fast pipeline for path-specific web measurement at scale},
  author = {Kirill},
  year   = {2026},
  url    = {https://github.com/Kirill89/webcensus}
}
```

## License

[MIT](LICENSE). Provided **as-is**, without warranty. Use at your own risk —
see the disclaimer at the top of this README.

## Configuration knobs

- `scripts/step3_probe_status.sh` — change `--path` to hunt a different file,
  bump `--concurrency` if your network can handle it.
- `scripts/step4_collect.sh` — fourth arg is the verifier: `text-file` (default)
  or `json`.
- `resolvers.txt` — add/remove resolvers; massdns load-balances across the list.
