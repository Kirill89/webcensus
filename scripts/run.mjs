#!/usr/bin/env bun

import {$} from 'bun';

const args = process.argv.slice(2);

let skipDomainsDownload = false;
let skipDns = false;
const positional = [];

for (const arg of args) {
    if (arg === '--skip-domains-download') skipDomainsDownload = true;
    else if (arg === '--skip-dns') skipDns = true;
    else if (arg === '-h' || arg === '--help') {
        console.log(`Usage: scripts/run.mjs <path> [verifier] [--skip-domains-download] [--skip-dns]

Example:
  ./scripts/run.mjs --skip-domains-download --skip-dns "/.well-known/security.txt" text-file

Arguments:
  <path>       HTTP path to probe (e.g. /.well-known/security.txt, /.git/config)
  [verifier]   Content verifier for step 6 (default: text-file). Options: text-file, json

Flags:
  --skip-domains-download   Skip step 1; reuse existing data/domains.txt
  --skip-dns                Skip step 2; reuse existing data/dns.ndjson`);
        process.exit(0);
    } else positional.push(arg);
}

const [path, verifier = 'text-file'] = positional;

if (!path) {
    console.error('Error: <path> argument required (e.g. /.well-known/security.txt).');
    console.error('Run with --help for usage.');
    process.exit(1);
}

const steps = [
    {
        name: 'step1: download domain list',
        skip: skipDomainsDownload,
        run: () => $`./scripts/step1_download_domain_list.sh`,
    },
    {
        name: 'step2: DNS resolution',
        skip: skipDns,
        run: () => $`./scripts/step2_massdns.sh`,
    },
    {
        name: 'step3: HTTPS status probe',
        skip: false,
        run: () => $`./scripts/step3_probe_status.sh ${path}`,
    },
    {
        name: 'step4: build curl config',
        skip: false,
        run: () => $`./scripts/step4_curl_config.mjs`,
    },
    {
        name: 'step5: bulk fetch',
        skip: false,
        // curl --parallel exits non-zero when any single URL fails (timeout,
        // size cap, refused, etc.). That is expected for mass scans — the
        // pipeline as a whole is fine as long as some files landed.
        allowFail: true,
        run: () => $`./scripts/step5_curl.sh`,
    },
    {
        name: 'step6: filter',
        skip: false,
        run: () => $`./scripts/step6_filter.mjs ${verifier}`,
    },
];

for (const step of steps) {
    if (step.skip) {
        console.log(`\n[skip] ${step.name}`);
        continue;
    }
    console.log(`\n[run]  ${step.name}`);

    const result = await step.run().nothrow();

    if (result.exitCode !== 0) {
        if (step.allowFail) {
            console.warn(`\n[warn] ${step.name} exited ${result.exitCode} (non-fatal, continuing)`);
            continue;
        }
        console.error(`\n[fail] ${step.name} (exit ${result.exitCode})`);
        process.exit(result.exitCode);
    }
}

console.log('\n[done] pipeline complete');
