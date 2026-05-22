#!/usr/bin/env node

import { readFileSync } from 'node:fs';

JSON.parse(readFileSync('package.json', 'utf8'));
JSON.parse(readFileSync('turbo.json', 'utf8'));

const gitignore = readFileSync('.gitignore', 'utf8');
for (const requiredEntry of ['.turbo/', '**/target/', '**/.dart_tool/', 'docker/.env']) {
  if (!gitignore.includes(requiredEntry)) {
    console.error(`Missing lint requirement: ${requiredEntry}`);
    process.exit(1);
  }
}

if (readFileSync('.nvmrc', 'utf8').trim() !== '20') {
  console.error('Expected .nvmrc to pin Node 20.');
  process.exit(1);
}
