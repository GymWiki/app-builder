// Minimal VM-service client (Node 22 global WebSocket)
const fs = require("fs");
function wsUri() {
  const m = fs.readFileSync("/tmp/exp/run.log", "utf8").match(/Debug service listening on (ws:\S+)/);
  return m && m[1];
}
async function connect() {
  const ws = new WebSocket(wsUri());
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0; const pend = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { const [res, rej] = pend.get(m.id); pend.delete(m.id); m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result); } };
  const call = (method, params = {}) => new Promise((res, rej) => { const i = ++id; pend.set(i, [res, rej]); ws.send(JSON.stringify({ id: i, method, params })); });
  const vm = await call("getVM");
  const iso = vm.isolates[0].id;
  const ext = (name, args = {}) => call("ext.flutter.inspector." + name, { isolateId: iso, ...args });
  return { ws, call, iso, ext, vm };
}
module.exports = { connect };
