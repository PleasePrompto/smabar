import { writeFileSync, appendFileSync } from 'node:fs';

export async function connect(socket) {
  const ws = new WebSocket('ws://127.0.0.1:9228' + socket);
  const pending = new Map();
  let next = 0, target;
  let ready;
  const connected = new Promise(resolve => { ready = resolve; });
  const events = [];
  ws.onmessage = ({data}) => {
    const message = JSON.parse(data);
    if (message.method === 'Target.targetCreated' && message.params.targetInfo.type === 'page') {
      target = message.params.targetInfo.targetId;
      ready();
    }
    if (message.method === 'Target.dispatchMessageFromTarget') {
      const inner = JSON.parse(message.params.message);
      if (inner.id) {
        const response = pending.get(inner.id);
        if (!response) return;
        pending.delete(inner.id);
        clearTimeout(response.timeout);
        if (inner.error) response.reject(new Error(JSON.stringify(inner.error)));
        else response.resolve(inner.result);
      } else events.push(inner);
    }
  };
  await connected;
  return {
    events,
    send(method, params={}) {
      const id = ++next;
      return new Promise((resolve,reject) => {
        const timeout = setTimeout(() => reject(new Error('Timed out: '+method)),15000);
        pending.set(id,{resolve,reject,timeout});
        ws.send(JSON.stringify({id:id+100000,method:'Target.sendMessageToTarget',params:{targetId:target,message:JSON.stringify({id,method,params})}}));
      });
    },
    close() { ws.close(); }
  };
}

if (import.meta.main) {
 const body = await (await fetch('http://127.0.0.1:9228/')).text();
 const sockets = [...body.matchAll(/\/socket\/\d+\/\d+\/WebPage/g)].map(m=>m[0]);
 for (const socket of sockets) {
  const client = await connect(socket);
  const info = await client.send('Runtime.evaluate',{expression:'JSON.stringify({label:window.__TAURI_INTERNALS__.metadata.currentWebview.label,nodes:document.querySelectorAll("*").length})',returnByValue:true});
  console.log(socket,JSON.stringify(info));
  await client.send('Memory.enable');
  await client.send('Memory.startTracking');
  await Bun.sleep(1200);
  await client.send('Memory.stopTracking');
  console.log('memory',JSON.stringify(client.events.filter(e=>e.method==='Memory.trackingUpdate').at(-1)));
  const heap = await client.send('Heap.snapshot');
  const parsed = JSON.parse(heap.snapshotData);
  const label = JSON.parse(info.result.value).label;
  writeFileSync(new URL('heap-before-'+label+'.json',import.meta.url),heap.snapshotData);
  console.log('snapshot', label, Object.keys(parsed),Object.fromEntries(Object.entries(parsed).filter(([k,v])=>Array.isArray(v)).map(([k,v])=>[k,v.length])));
  client.close();
 }
}
