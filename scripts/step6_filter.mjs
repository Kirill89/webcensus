#!/usr/bin/env bun

import fs from 'node:fs';
import path from 'node:path';

const verifier = process.argv[2] || 'text-file';
const input = 'data/unfiltered';
const output = 'data/filtered';

fs.mkdirSync(output, {recursive: true, existOk: true});

const verifiers = {
    'text-file': (text) => {
        const clean = text.toLowerCase().replace(/\s/g, '');

        // Filter out empty files various `Not Found`, `empty OK`, etc.
        if (clean.length < 16) {
            return false;
        }

        // Filter out unprintable characters (binary?).
        if (/\p{C}/u.test(clean)) {
            return false;
        }

        // Reject files that weren't valid UTF-8: Node replaces invalid sequences
        // with U+FFFD, which slips past the \p{C} control-char check below.
        if (text.includes('\ufffd')) {
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
    },
    'json': (text) => {
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
    },
};

const verify = verifiers[verifier];

console.log('Verifying files...', verifier);

for (let fileName of fs.readdirSync(input)) {
    const text = fs.readFileSync(path.join(input, fileName), 'utf-8');

    if (verify(text)) {
        fs.writeFileSync(path.join(output, fileName), text);
        console.log('✅', fileName);
    } else {
        console.log('❌', fileName);
    }
}
