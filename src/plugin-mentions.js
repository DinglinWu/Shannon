const boundary = '[\\s，。！？、：；（）(){}\\[\\]<>"\'`.:]';
const prefix = '[\\s，。！？、：；（(\\p{Script=Han}]';
const tokenPattern = () => new RegExp(`(^|${prefix})[@＠]([^@＠${boundary.slice(1,-1)}]+)`, 'gu');

// Backticks/fenced examples are literal text, not plugin selections.
function codeRanges(text) {
  const ranges = []; let opened = null;
  const runs = /`+|^ {0,3}~{3,}/gm;
  for (const match of text.matchAll(runs)) {
    const mark = match[0].trim();
    if (!opened) opened = {mark,start:match.index};
    else if (opened.mark === mark) { ranges.push([opened.start,match.index+match[0].length]);opened=null; }
  }
  if (opened) ranges.push([opened.start,text.length]);
  const codeOnly = [...ranges]; let rangeIndex = 0;
  let quote = null;
  const closes = {'"':'"',"'":"'",'“':'”','‘':'’'};
  for (let i=0;i<text.length;i++) {
    while (rangeIndex < codeOnly.length && i >= codeOnly[rangeIndex][1]) rangeIndex++;
    if (rangeIndex < codeOnly.length && i >= codeOnly[rangeIndex][0]) {i=codeOnly[rangeIndex][1]-1;continue;}
    if (text[i]==='\\') {i++;continue;}
    if (quote) {
      if (text[i]===quote.close) {ranges.push([quote.start,i+1]);quote=null;}
    } else if (closes[text[i]] && (text[i]!=="'" || i===0 || /[\s([{（]/u.test(text[i-1]))) {
      quote={start:i,close:closes[text[i]]};
    }
  }
  if (quote) ranges.push([quote.start,text.length]);
  const merged=[];
  for (const range of ranges.sort((a,b)=>a[0]-b[0])) {
    const last=merged.at(-1);
    if (last && range[0] <= last[1]) last[1]=Math.max(last[1],range[1]);
    else merged.push([...range]);
  }
  return merged;
}
function inCode(ranges,index) {
  let lo=0, hi=ranges.length-1;
  while(lo<=hi){const mid=(lo+hi)>>1;const [start,end]=ranges[mid];if(index<start)hi=mid-1;else if(index>=end)lo=mid+1;else return true;}
  return false;
}
export function mentionTokens(text) {
  const ranges = codeRanges(text), result = [];
  for (const match of text.matchAll(tokenPattern())) {
    const start = match.index + match[1].length;
    if (!inCode(ranges,start)) result.push({start,end:match.index+match[0].length,name:match[2]});
  }
  return result;
}
export function mentionContext(text, caret = text.length) {
  if (inCode(codeRanges(text), Math.max(0,caret-1))) return null;
  const before = text.slice(0,caret);
  const match = before.match(new RegExp(`(^|${prefix})[@＠]([^@＠${boundary.slice(1,-1)}]*)$`, 'u'));
  if (!match) return null;
  const start = match.index + match[1].length;
  const tail = text.slice(caret).match(new RegExp(`^[^@＠${boundary.slice(1,-1)}]*`, 'u'))[0];
  return {start,end:caret+tail.length,query:match[2]};
}
function resolve(name, plugins) {
  const explicitPlugin = name.startsWith('plugin/');
  const [id, tool, ...extra] = (explicitPlugin ? name.slice(7) : name).split('/');
  if (extra.length || !id || tool === '') return null;
  const enabled = plugins.filter(p => p.enabled && (!explicitPlugin || !p.builtin));
  const builtins = !explicitPlugin ? enabled.filter(p=>p.builtin && [p.manifest.id,p.manifest.name,...(p.manifest.aliases||[])].some(n=>n.toLowerCase()===id.toLowerCase())) : [];
  let matches = builtins.length ? builtins : enabled.filter(p => p.manifest.id.toLowerCase() === id.toLowerCase());
  if (!matches.length) matches = enabled.filter(p => [p.manifest.name,...(p.manifest.aliases || [])].some(n=>n.toLowerCase()===id.toLowerCase()));
  if (!explicitPlugin && matches.some(p=>p.builtin)) matches=matches.filter(p=>p.builtin);
  if (matches.length !== 1) return null;
  const plugin = matches[0];
  if (tool && !plugin.manifest.tools.some(t => t.name === tool)) return null;
  return {id:plugin.manifest.id,...(tool ? {tool} : {}),...(plugin.builtin ? {builtin:true} : {})};
}
export function mentionCandidates(plugins, query) {
  const q = query.toLowerCase(), roots = [], actions = [];
  for (const item of plugins) {
    const {manifest:m,enabled,builtin}=item;
    if (!enabled) continue;
    const collision=!builtin && (m.id==='plugin' || plugins.some(p=>p.builtin && [p.manifest.id,...(p.manifest.aliases||[])].includes(m.id)));
    const id=collision ? `plugin/${m.id}` : m.id;
    if (!q || [id,m.name,...(m.aliases||[])].some(n=>n.toLowerCase().includes(q))) {
      roots.push({fill:`@${id}`,name:`@${id}`,label:m.name,description:m.description,tag:builtin?'内置':''});
    }
    for (const t of m.tools) {
      const name = `${id}/${t.name}`;
      if (q && !`${name} ${m.name} ${t.description}`.toLowerCase().includes(q)) continue;
      actions.push({fill:`@${name}`,name:`@${name}`,label:m.name,description:t.description,tag:builtin?'内置':''});
    }
  }
  return [...roots,...actions];
}
export function replaceMention(text, caret, fill) {
  const context = mentionContext(text,caret);
  if (!context) return null;
  const suffix = text.slice(context.end);
  const inserted = fill + ' ';
  return {text:text.slice(0,context.start)+inserted+(suffix.startsWith(' ') ? suffix.slice(1) : suffix),caret:context.start+inserted.length};
}
export function selectedMentionRanges(text,plugins) {
  return mentionTokens(text).filter(t => resolve(t.name,plugins));
}
export function parsePluginMentions(text,plugins) {
  const tokens = mentionTokens(text), targets = [];
  for (const token of tokens) {
    const target = resolve(token.name,plugins);
    if (!target) throw new Error(`@${token.name} 不存在、名称不唯一或插件已停用，请从 @ 列表中重新选择。`);
    if (!targets.some(t => t.id===target.id && t.tool===target.tool && !!t.builtin===!!target.builtin)) targets.push(target);
  }
  if (targets.length > 8) throw new Error('一次最多选择8个插件或工具');
  // A bare @ is an unfinished picker, never a request to the model.
  if (mentionContext(text,text.length)?.query === '') throw new Error('请先从 @ 列表选择插件或工具');
  let body = '', end = 0;
  for (const token of tokens) {body += text.slice(end,token.start);end=token.end;}
  body = (body+text.slice(end)).trim();
  if (!targets.length) return {targets,body:text};
  if (!body) {
    if (targets.length===1 && targets[0].builtin) {
      const target=targets[0], item=plugins.find(p=>p.builtin && p.manifest.id===target.id);
      const tool=target.tool || item.empty_tool;
      const spec=item.manifest.tools.find(t=>t.name===tool);
      if (spec && (!spec.parameters.required?.length || (target.id==='clock' && tool==='open'))) {
        return {targets,body,direct:{action:'builtin',id:target.id,tool,args:{}}};
      }
    }
    throw new Error('已选中工具，请在 @ 名称后输入你的需求');
  }
  if(targets.length===1&&targets[0].builtin&&targets[0].id==='download'&&!targets[0].tool&&/^https?:\/\//i.test(body))return{targets,body,direct:{action:'builtin',id:'download',tool:'prepare',args:{url:body}}};
  if(targets.length===1&&targets[0].builtin&&targets[0].id==='wechat'&&!body.startsWith('{'))return{targets,body,direct:{action:'builtin',id:'wechat',tool:'open',args:{peer_name:body}}};
  // Structured arguments remain a direct, offline shortcut. Natural language
  // goes to the existing AI tool loop, with an explicit backend tool scope.
  let direct = null;
  if (targets.length === 1 && body.startsWith('{')) {
    let args;
    try {args=JSON.parse(body);} catch {throw new Error('结构化参数不是有效 JSON，也可以直接用自然语言描述需求');}
    if (!args || Array.isArray(args) || typeof args!=='object') throw new Error('参数须为 JSON 对象');
    const target=targets[0], plugin=plugins.find(p=>p.manifest.id===target.id && !!p.builtin===!!target.builtin);
    const tool=target.tool || plugin.default_tool || (plugin.manifest.tools.length===1 ? plugin.manifest.tools[0].name : null);
    if (!tool) throw new Error('该插件包含多个工具，请先使用 @插件ID/工具名 选择一个工具');
    direct={action:target.builtin ? 'builtin' : 'run',id:target.id,tool,args};
  }
  return {targets,body,direct};
}

export function removeMention(text,range,start,end=start) {
  if(!Number.isInteger(range.start)||!Number.isInteger(range.end)||range.start<0||range.end>text.length||range.end<=range.start)throw new Error('工具标记已变化');
  const gap=Math.min(11,(text.slice(range.end).match(/^ */)||[''])[0].length);
  const until=range.end+gap,length=until-range.start;
  const adjust=position=>position<=range.start?position:position<until?range.start:position-length;
  return {text:text.slice(0,range.start)+text.slice(until),start:adjust(start),end:adjust(end)};
}

// Reserve actual whitespace for the dismiss control so mirror and caret agree.
export function spaceMentionControls(text,ranges,start,end=start){
 let value=text,a=start,b=end;
 for(const range of [...ranges].reverse()){
  const existing=(text.slice(range.end).match(/^ */)||[''])[0].length;
  const count=Math.max(0,11-existing);if(!count)continue;
  const at=range.end+existing;value=value.slice(0,at)+' '.repeat(count)+value.slice(at);
  if(a>=at)a+=count;if(b>=at)b+=count;
 }
 return {text:value,start:a,end:b};
}
