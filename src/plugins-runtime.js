// Plugin code runs only in a dedicated Worker owned by an opaque sandbox.
// Neither the worker nor its iframe receives Tauri bindings or native tokens.
const workerPrelude = `
let tools = Object.create(null), callId = 0;
const waiting = new Map();
const send = self.postMessage.bind(self);
self.Shannon = Object.freeze({
  register(value) {
    if (!value || typeof value !== 'object') throw new Error('Shannon.register expects a tool map');
    tools = value;
  },
  call(method, args = {}) {
    const id = ++callId;
    return new Promise((resolve, reject) => {
      waiting.set(id, {resolve, reject});
      send({kind:'host', id, method, args});
    });
  }
});
self.onmessage = async ({data}) => {
  if (data.kind === 'host-result') {
    const entry = waiting.get(data.id); if (!entry) return;
    waiting.delete(data.id);
    data.ok ? entry.resolve(data.data) : entry.reject(new Error(data.error));
  } else if (data.kind === 'run') {
    try {
      if (!Object.hasOwn(tools, data.tool) || typeof tools[data.tool] !== 'function') throw new Error('插件未注册该工具');
      const value = await tools[data.tool](data.args);
      const encoded = JSON.stringify(value === undefined ? null : value);
      if (new TextEncoder().encode(encoded).byteLength > 60000) throw new Error('插件结果过大');
      send({kind:'result', output:{ok:true, data:JSON.parse(encoded)}});
    } catch(error) { send({kind:'result',output:{ok:false,error:String(error?.message || error).slice(0,1000)}}); }
  }
};
try {
`;
const workerSuffix = `
} catch (error) {
  self.postMessage({kind:'result',output:{ok:false,error:String(error?.message || error).slice(0,1000)}});
}
`;
export function sandboxDocument(code, nonce) {
  const worker = workerPrelude + '\n' + code + '\n' + workerSuffix;
  const bootstrap = `
    const worker = new Worker(URL.createObjectURL(new Blob([${JSON.stringify(worker).replace(/</g, '\\u003c')}], {type:'text/javascript'})));
    worker.onmessage = ({data}) => parent.postMessage(data, '*');
    worker.onerror = () => parent.postMessage({kind:'result',output:{ok:false,error:'插件脚本加载或运行失败'}}, '*');
    addEventListener('message', event => {
      if (event.source !== parent) return;
      if (event.data?.kind === 'cancel') {worker.terminate(); return;}
      if (event.data?.kind === 'run' || event.data?.kind === 'host-result') worker.postMessage(event.data);
    });
    parent.postMessage({kind:'ready'}, '*');
  `;
  return `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; worker-src blob:; connect-src 'none'; base-uri 'none'; form-action 'none'"><script nonce="${nonce}">${bootstrap}<\/script>`;
}

export async function initPluginRuntime({ invoke, listen }) {
  const active = new Map();
  function dispose(request) {
    const entry = active.get(request); if (!entry) return;
    active.delete(request);
    clearTimeout(entry.timer);
    window.removeEventListener('message', entry.receive);
    entry.frame.contentWindow?.postMessage({kind:'cancel'}, '*');
    entry.frame.remove();
  }
  await listen('plugin-cancel', ({payload}) => dispose(payload.request));
  await listen('plugin-execute', ({payload}) => {
    if (typeof payload?.request !== 'string' || active.has(payload.request)) return;
    const { request, tool, args, code } = payload;
    if (active.size >= 4 || typeof code !== 'string' || code.length > 524288) {
      invoke('plugin_result', {request,output:{ok:false,error:'插件运行数量或脚本大小超限'}}).catch(() => {}); return;
    }
    const frame = document.createElement('iframe');
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.setAttribute('aria-hidden', 'true');
    frame.setAttribute('allow', "camera 'none'; microphone 'none'; clipboard-read 'none'; clipboard-write 'none'; geolocation 'none'; display-capture 'none'; usb 'none'; serial 'none'");
    frame.hidden = true;
    frame.srcdoc = sandboxDocument(code, crypto.randomUUID());
    let completed = false, started = false, messages = 0, hostCalls = 0;
    async function finish(output) {
      if (completed) return;
      completed = true;
      dispose(request);
      try { await invoke('plugin_result', {request, output}); } catch { /* expired native request */ }
    }
    async function receive(event) {
      if (event.source !== frame.contentWindow || completed) return;
      if (++messages > 128) { await finish({ok:false,error:'插件消息数量超限'}); return; }
      const data = event.data;
      if (!data || typeof data !== 'object') return;
      if (data.kind === 'ready' && !started) { started = true; frame.contentWindow.postMessage({kind:'run', tool, args}, '*'); }
      else if (data.kind === 'result') {
        try {
          const output = data.output;
          if (!output || typeof output.ok !== 'boolean' || new TextEncoder().encode(JSON.stringify(output)).byteLength > 60000) throw new Error();
          await finish(output);
        } catch { await finish({ok:false,error:'插件返回格式无效或结果过大'}); }
      } else if (data.kind === 'host' && Number.isSafeInteger(data.id) && typeof data.method === 'string') {
        if (++hostCalls > 32) { await finish({ok:false,error:'插件宿主调用数量超限'}); return; }
        let reply;
        try {
          if (new TextEncoder().encode(JSON.stringify(data.args ?? {})).byteLength > 32768) throw new Error('插件宿主参数超过32KiB');
          reply = {ok:true,data:await invoke('plugin_host_call', {request,method:data.method,args:data.args ?? {}})};
        }
        catch(error) { reply = {ok:false,error:String(error).slice(0,1000)}; }
        if (!completed) frame.contentWindow?.postMessage({kind:'host-result',id:data.id,...reply}, '*');
      }
    }
    const timer = setTimeout(() => finish({ok:false,error:'outcome_unknown:插件执行超时，修改可能已经完成，请先查询状态'}), 10500);
    active.set(request, {frame, receive, timer});
    window.addEventListener('message', receive);
    document.body.append(frame);
  });
}
