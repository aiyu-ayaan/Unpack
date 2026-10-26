#!/usr/bin/env node
// Writes one version into every manifest, so the app, the bundles and the updater's check agree.
//   node scripts/set-version.mjs 0.0.1-alpha.1
import { readFileSync, writeFileSync } from 'node:fs'

const version = process.argv[2]
if (!/^\d+\.\d+\.\d+(-(alpha|beta)\.\d+)?$/.test(version ?? '')) throw new Error(`Invalid version '${version}'.`)

const edit = (path, change) => writeFileSync(path, change(readFileSync(path, 'utf8')))
const json = (path, change) => edit(path, (text) => JSON.stringify(change(JSON.parse(text)), null, 2) + '\n')
// Fails loudly when a manifest no longer has the line, rather than releasing a stale version.
const replace = (text, pattern, value, path) => {
  if (!pattern.test(text)) throw new Error(`${path}: nothing matches ${pattern}`)
  return text.replace(pattern, value)
}

json('app/package.json', (p) => ({ ...p, version }))
json('app/src-tauri/tauri.conf.json', (p) => ({ ...p, version }))
edit('app/src-tauri/Cargo.toml', (t) => replace(t, /(\[package\]\nname = "unpack"\nversion = ")[^"]*/, `$1${version}`, 'Cargo.toml'))
edit('app/src-tauri/Cargo.lock', (t) => replace(t, /(name = "unpack"\nversion = ")[^"]*/, `$1${version}`, 'Cargo.lock'))
console.log(`version ${version}`)
