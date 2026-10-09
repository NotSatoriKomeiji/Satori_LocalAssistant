// Check the actual executable contains the current HTML, JS and CSS payloads.
// Run after a Windows release build: node scripts/verify-release.mjs <exe-path>
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, dirname, join, extname } from 'node:path';
import { brotliDecompressSync } from 'node:zlib';
import assert from 'node:assert/strict';
const executable = resolve(process.argv[2] ?? 'target/release/satori.exe');
const binary = readFileSync(executable);
const build = resolve(process.argv[3] ?? join(dirname(executable), 'build'));
const html = readFileSync('dist/index.html', 'utf8');
const dependencies = [...html.matchAll(/(?:src|href)="(\/assets\/[^"?#]+)"/g)].map(m => m[1]);
assert(dependencies.some(p => p.endsWith('.js')) && dependencies.some(p => p.endsWith('.css')));
function decode(data) { try { return brotliDecompressSync(data); } catch { return data; } }
let verified = false;
for (const entry of readdirSync(build)) {
  if (!entry.startsWith('habitos-desktop-')) continue;
  const folder = join(build, entry, 'out', 'tauri-codegen-assets');
  if (!existsSync(folder)) continue;
  const assets = readdirSync(folder).map(name => {
    const data = readFileSync(join(folder, name));
    return { name, data, content: decode(data) };
  });
  const index = assets.find(a => extname(a.name) === '.html'
    && dependencies.every(p => a.content.includes(p)) && binary.includes(a.data));
  if (!index) continue;
  assert(!index.content.includes('@vite/client') && !index.content.includes('127.0.0.1:1420'));
  for (const dependency of dependencies) {
    const expected = readFileSync(join('dist', dependency));
    assert(assets.some(a => a.content.equals(expected) && binary.includes(a.data)),
      `Executable is missing the current ${dependency}`);
  }
  verified = true;
  break;
}
assert(verified, 'Executable has no embedded production HTML; it may depend on a development server');
// The Windows resource compiler must rerun when icons change, too.
const icon = readFileSync('src-tauri/icons/icon.ico');
const count = icon.readUInt16LE(4);
for (let i = 0; i < count; i++) {
  const entry = 6 + i * 16;
  const size = icon.readUInt32LE(entry + 8);
  const offset = icon.readUInt32LE(entry + 12);
  assert(binary.includes(icon.subarray(offset, offset + size)),
    `Executable is missing current icon frame ${i}; rebuild Windows resources`);
}
console.log(`Verified embedded production HTML, ${dependencies.length} frontend dependencies and ${count} icon sizes in ${executable}`);
