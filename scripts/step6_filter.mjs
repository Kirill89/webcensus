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

        // Reject files that weren't valid UTF-8: Node replaces invalid sequences
        // with U+FFFD, which slips past the \p{C} control-char check below.
        if (text.includes('\ufffd')) {
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
            !clean.includes('<imgsrc=') &&
            !clean.includes('</center>') &&
            !clean.includes('</table>') &&
            !clean.includes('</b>') &&
            !clean.includes('</b>') &&
            !clean.includes('[0]=>') &&
            !clean.includes(':require():') &&
            !clean.includes(':include():') &&
            !clean.includes('#ext-x-stream-inf:') &&
            !clean.includes('</a>') &&
            !clean.includes('<br/>') &&
            !clean.includes('<br>') &&
            !clean.includes('</script>') &&
            !clean.includes('</span>') &&
            !clean.includes('</pre>') &&
            !clean.includes('</font>') &&
            !clean.includes('</style>') &&
            !clean.includes('</iframe>') &&
            !clean.includes('<?php') &&
            !clean.includes('<?xml') &&
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
    'env': (text) => {
        if (!verifiers['text-file'](text)) {
            return false;
        }

        return text.includes('=');
    },
    'security.txt': (text) => {
        if (!verifiers['text-file'](text)) {
            return false;
        }

        const clean = text.toLowerCase().replace(/\s/g, '');

        return clean.includes('contact:') || clean.includes('expires:');
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
