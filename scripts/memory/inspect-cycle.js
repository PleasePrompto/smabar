// Flyout cycles under WebKit's own memory accounting: category sizes per
// cycle, DOM counts per page, heap class deltas before/after/after-GC.
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
}
const bar = pages.bar;
if (!bar) throw new Error('bar page missing: ' + Object.keys(pages));
const domCounts = async () => {
  const out = {};
  for (const [name, client] of Object.entries(pages)) {
    const r = await client.send('Runtime.evaluate', {expression: 'JSON.stringify({nodes: document.querySelectorAll("*").length, shadow: [...document.querySelectorAll("*")].filter(e => e.shadowRoot).length, sheets: document.styleSheets.length})', returnByValue: true});
    out[name] = JSON.parse(r.result.value);
  }
  return out;
};
const summarize = (data) => {
  const s = JSON.parse(data); const classes = {}; let size = 0;
  for (let i = 0; i < s.nodes.length; i += 4) {
    const bytes = s.nodes[i + 1]; const name = s.nodeClassNames[s.nodes[i + 2]];
    size += bytes; const b = classes[name] ??= {count: 0, bytes: 0}; b.count++; b.bytes += bytes;
  }
  return {nodes: s.nodes.length / 4, size, classes};
};
const categories = () => Object.fromEntries((bar.events.filter(e => e.method === 'Memory.trackingUpdate').at(-1)?.params.event.categories ?? []).map(c => [c.type, c.size]));
const timeline = [];
const mark = async (phase) => {
  const row = {phase, t: Date.now(), categories: categories(), dom: await domCounts()};
  timeline.push(row); console.log(JSON.stringify(row));
};
await bar.send('Memory.enable');
await bar.send('Memory.startTracking');
await Bun.sleep(2000);
await mark('start');
const before = summarize((await bar.send('Heap.snapshot')).snapshotData);
const tiles = ['plugin:clock:clock', 'plugin:systeminfo:system', 'plugin:weather:weather', 'plugin:todos:todos'];
const mcp = fileURLToPath(new URL('mcp-call.py', base));
for (let step = 0; step < cycles; step++) {
  await Bun.$`python3 ${mcp} open_flyout ${tiles[step % 4]}`.quiet();
  await Bun.sleep(2000);
  await Bun.$`python3 ${mcp} close_flyout`.quiet();
  await Bun.sleep(8000);
  await mark(`cycle-${step + 1}`);
}
const after = summarize((await bar.send('Heap.snapshot')).snapshotData);
await bar.send('Heap.gc');
await Bun.sleep(5000);
await mark('after-gc');
const settled = summarize((await bar.send('Heap.snapshot')).snapshotData);
await bar.send('Memory.stopTracking');
const delta = (a, b) => Object.entries(b.classes).map(([name, v]) => ({name, count: v.count - (a.classes[name]?.count ?? 0), bytes: v.bytes - (a.classes[name]?.bytes ?? 0)})).filter(d => d.count !== 0).sort((x, y) => y.bytes - x.bytes);
const report = {label, cycles, timeline,
  heap: {before: {nodes: before.nodes, size: before.size}, after: {nodes: after.nodes, size: after.size}, afterGc: {nodes: settled.nodes, size: settled.size},
    topDeltaAfter: delta(before, after).slice(0, 15), topDeltaAfterGc: delta(before, settled).slice(0, 15)},
  updates: bar.events.filter(e => e.method === 'Memory.trackingUpdate').map(e => Object.fromEntries(e.params.event.categories.map(c => [c.type, c.size])))};
writeFileSync(new URL(`${label}-inspector.json`, out), JSON.stringify(report, null, 1));
console.log('heap', JSON.stringify(report.heap));
for (const client of Object.values(pages)) client.close();
