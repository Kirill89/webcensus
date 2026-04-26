#!/usr/bin/env bun

import readline from 'node:readline';
import fs from 'node:fs';

const input = 'data/status.ndjson';
const output = 'data/urls.curl';

const rl = readline.createInterface({
    input: fs.createReadStream(input),
    crlfDelay: Infinity,
});

let totalAllCodes = 0;
let total200 = 0;

const fd = fs.openSync(output, 'w');

for await (const line of rl) {
    try {
        const data = JSON.parse(line);

        if (data.cert_ok === true && data.status === 'success') {
            totalAllCodes++;

            if (data.code === 200) {
                const fileName = data.url.replace('https://', '').replace(/[^a-zA-Z0-9.-]/g, '_');

                total200++;
                fs.writeSync(fd, `url = "${data.url}"\noutput = "${fileName}"\n\n`);
            }
        }
    } catch (_) {
    }
}

fs.closeSync(fd);

console.log('Total 200:', total200, 'Total all codes:', totalAllCodes);
