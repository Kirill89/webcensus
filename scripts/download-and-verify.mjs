#!/usr/bin/env bun

import readline from 'node:readline';
import fs from 'node:fs';
import path from 'node:path';

const input = process.argv[2];
const output = process.argv[3];
const verifier = process.argv[4];
let skip = parseInt(process.argv[5], 10) || 0;

fs.mkdirSync(output, {recursive: true, existOk: true});

const rl = readline.createInterface({
    input: fs.createReadStream(input),
    crlfDelay: Infinity,
});

const urls = new Set();

for await (const line of rl) {
    try {
        const data = JSON.parse(line);

        if (data.cert_ok === true && data.code === 200 && data.status === 'success') {
            urls.add(data.url);
        }
    } catch (_) {
    }
}

console.log('Total:', urls.size);

function verify(text) {
    if (verifier === 'text-file' || !verifier) {
        const clean = text.toLowerCase().replace(/\s/g, '');

        // Filter out empty files various `Not Found`, `empty OK`, etc.
        if (clean.length < 16) {
            return false;
        }

        // Filter out unprintable characters (binary?).
        if (/\p{C}/u.test(clean)) {
            return false;
        }

        return !clean.includes('<!doctype') &&
            !clean.includes('</body>') &&
            !clean.includes('<html>') &&
            !clean.includes('<title>') &&
            !clean.includes('</p>') &&
            !clean.includes('</b>') &&
            !clean.includes('</b>') &&
            !clean.includes('<br/>') &&
            !clean.includes('<br>') &&
            !clean.includes('<?php') &&
            !clean.includes('<metahttp-equiv') &&
            !clean.startsWith('{"') &&
            !clean.includes('(function(') &&
            !clean.includes('</div>');
    } else if (verifier === 'json') {
        if (text.trim().startsWith('{') || text.trim().startsWith('[')) {
            try {
                JSON.parse(text);
                return true;
            } catch (_) {
                return false;
            }
        } else {
            return false;
        }
    } else if (verifier === 'json') {
        const clean = text.trim();

        if (!clean.startsWith('{') && !clean.startsWith('[')) {
            return false;
        }

        try {
            JSON.parse(text);
            return true;
        } catch (_) {
            return false;
        }
    }

    throw new Error('Not valid verifier');
}

let i = skip;
for (let url of urls) {
    if (skip-- > 0) continue;

    const fileName = url.replace('https://', '').replace(/[^a-zA-Z0-9.-]/g, '_');
    const filePath = path.join(output, fileName);

    if (fs.existsSync(filePath)) {
        continue;
    }

    try {
        const response = await fetch(url, {
            signal: AbortSignal.timeout(5000),
        });
        const text = await response.text();

        if (verify(text)) {
            fs.writeFileSync(filePath, text);
        } else {
            console.log('Verification failed:', url, text.slice(0, 30));
        }
    } catch (e) {
        console.error(e);
    }

    console.log('Progress:', ++i, '/', urls.size);
}
