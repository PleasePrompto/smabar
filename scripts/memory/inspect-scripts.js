// Which JavaScript programs run per flyout cycle, on which page, how big:
// Debugger.scriptParsed on every page (evaluated programs carry the page URL;
// the probe's own Runtime.evaluate has an empty URL and is skipped),
// aggregated by event name (Tauri emit template) or source prefix.
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { connect } from './inspect.js';
const base = new URL('./', import.meta.url);
const out = process.env.SMABAR_MEMORY_OUT ? new URL(process.env.SMABAR_MEMORY_OUT.replace(/\/?$/, '/'), 'file://') : new URL('../../.debug/memory/', import.meta.url);
const label = process.argv[2];
const cycles = Number(process.argv[3] ?? 12);
if (!label || !/^[a-z0-9-]+$/.test(label)) throw new Error('label required');
const body = await (await fetch('http://127.0.0.1:9228/')).text();
const sockets = [...new Set([...body.matchAll(/\/socket\/\d+\/\d+\/WebPage/g)].map(m => m[0]))];
const pages = {};
for (const socket of sockets) {
  const client = await connect(socket);
  const info = await client.send('Runtime.evaluate', {expression: 'window.__TAURI_INTERNALS__.metadata.currentWebview.label', returnByValue: true});
  pages[info.result.value] = client;
  await client.send('Debugger.enable');
  await client.send('Network.enable');
  client.events.length = 0;
}
const classify = (source) => {
  const event = source.match(/\{event: '([^']+)'/);
  if (event) return `emit:${event[1]}`;
  return 'other:' + source.slice(0, 70).replace(/\s+/g, ' ');
};
const phases = [];
const mark = async (phase) => {
  const row = {phase, pages: {}};
  for (const [name, client] of Object.entries(pages)) {
    const parsed = client.events.filter(e => e.method === 'Debugger.scriptParsed' && e.params.url !== '');
    const buckets = {};
    for (const e of parsed) {
      const {scriptSource} = await client.send('Debugger.getScriptSource', {scriptId: e.params.scriptId});
      const key = classify(scriptSource);
      const b = buckets[key] ??= {count: 0, bytes: 0};
      b.count++; b.bytes += scriptSource.length;
    }
    const requests = client.events.filter(e => e.method === 'Network.requestWillBeSent').map(e => e.params.request.url.replace(/\?.*$/, ''));
    const urls = {};
    for (const url of requests) urls[url] = (urls[url] ?? 0) + 1;
    client.events.length = 0;
    row.pages[name] = {programs: parsed.length, bytes: Object.values(buckets).reduce((s, b) => s + b.bytes, 0), buckets, requests: requests.length, urls};
  }
  phases.push(row);
  console.log(JSON.stringify({phase, summary: Object.fromEntries(Object.entries(row.pages).map(([n, p]) => [n, `${p.programs} programs, ${(p.bytes / 1024).toFixed(1)} KiB, ${p.requests} requests`]))}));
};
const tiles = ['plugin:clock:clock', 'plugin:systeminfo:system', 'plugin:weather:weather', 'plugin:todos:todos'];
const mcp = fileURLToPath(new URL('mcp-call.py', base));
await Bun.sleep(10000);
await mark('idle-10s');
for (let step = 0; step < cycles; step++) {
  await Bun.$`python3 ${mcp} open_flyout ${tiles[step % 4]}`.quiet();
  await Bun.sleep(2000);
  await Bun.$`python3 ${mcp} close_flyout`.quiet();
  await Bun.sleep(8000);
  await mark(`cycle-${step + 1}`);
}
const totals = {};
for (const row of phases.slice(1)) for (const [page, p] of Object.entries(row.pages)) for (const [key, b] of Object.entries(p.buckets)) {
  const t = totals[`${page} ${key}`] ??= {count: 0, bytes: 0}; t.count += b.count; t.bytes += b.bytes;
}
const ranked = Object.entries(totals).sort((a, b) => b[1].bytes - a[1].bytes);
const urlTotals = {};
for (const row of phases.slice(1)) for (const [page, p] of Object.entries(row.pages)) for (const [url, n] of Object.entries(p.urls)) urlTotals[`${page} ${url}`] = (urlTotals[`${page} ${url}`] ?? 0) + n;
writeFileSync(new URL(`${label}-scripts.json`, out), JSON.stringify({label, cycles, phases, totals: ranked, urlTotals}, null, 1));
console.log('requests over all cycles (page url: count):');
for (const [key, n] of Object.entries(urlTotals).sort((a, b) => b[1] - a[1]).slice(0, 12)) console.log(`  ${key.slice(0, 120)}: ${n}`);
console.log('top programs over all cycles (page key: count, KiB):');
for (const [key, t] of ranked.slice(0, 15)) console.log(`  ${key.slice(0, 110)}: ${t.count}, ${(t.bytes / 1024).toFixed(1)}`);
for (const client of Object.values(pages)) client.close();
