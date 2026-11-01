export const COMPACT = 64;
export function areaOf(m) {
  const a=m.workArea || {position:m.position,size:m.size};
  return {x:a.position.x,y:a.position.y,width:a.size.width,height:a.size.height,scale:m.scaleFactor||1};
}
export function monitorFor(point,monitors) {
  if (!monitors.length) throw new Error('未找到可用显示器');
  let best=monitors[0], distance=Infinity;
  for(const m of monitors){const a=areaOf(m);const dx=Math.max(a.x-point.x,0,point.x-a.x-a.width),dy=Math.max(a.y-point.y,0,point.y-a.y-a.height);const d=dx*dx+dy*dy;if(d<distance){distance=d;best=m;}}
  return best;
}
export function compactRect(anchor,monitors) {
  const m=monitorFor({x:anchor.x+1,y:anchor.y+1},monitors),a=areaOf(m),size=Math.round(COMPACT*a.scale);
  return {x:Math.round(Math.max(a.x,Math.min(anchor.x,a.x+a.width-size))),y:Math.round(Math.max(a.y,Math.min(anchor.y,a.y+a.height-size))),width:size,height:size,scale:a.scale};
}
export function snapAnchor(anchor,monitors,threshold=72) {
  const r=compactRect(anchor,monitors),a=areaOf(monitorFor({x:r.x+1,y:r.y+1},monitors));
  const edges=[['left',r.x-a.x],['right',a.x+a.width-r.x-r.width],['top',r.y-a.y],['bottom',a.y+a.height-r.y-r.height]].sort((a,b)=>a[1]-b[1]);
  let edge=edges[0][1]<=threshold*r.scale?edges[0][0]:'free';
  if(edge==='left')r.x=a.x;if(edge==='right')r.x=a.x+a.width-r.width;if(edge==='top')r.y=a.y;if(edge==='bottom')r.y=a.y+a.height-r.height;
  return {...r,edge};
}
export function expandedRect(anchor,monitors) {
  const compact=compactRect(anchor,monitors),a=areaOf(monitorFor({x:compact.x+1,y:compact.y+1},monitors));
  const width=Math.min(a.width,Math.round(380*a.scale)),height=Math.min(a.height,Math.round(460*a.scale));
  const x=Math.max(a.x,Math.min(compact.x,a.x+a.width-width)),y=Math.max(a.y,Math.min(compact.y,a.y+a.height-height));
  return {x,y,width,height,scale:a.scale,fx:width===compact.width?0:(compact.x-x)/(width-compact.width),fy:height===compact.height?0:(compact.y-y)/(height-compact.height)};
}
export function orderedBefore(ids,id,before) {
  if(!ids.includes(id)||(before!==null&&!ids.includes(before)))throw new Error('待办顺序已变化');
  if(id===before)return [...ids];const result=ids.filter(n=>n!==id);const index=before===null?result.length:result.indexOf(before);result.splice(index,0,id);return result;
}
export function localDate(now=new Date()) {return `${now.getFullYear()}-${String(now.getMonth()+1).padStart(2,'0')}-${String(now.getDate()).padStart(2,'0')}`;}
export function dateCaption(date,now=new Date()) {
  if(!date)return '';
  if(date===localDate(now))return '今天';
  const tomorrow=new Date(now.getFullYear(),now.getMonth(),now.getDate()+1);
  if(date===localDate(tomorrow))return '明天';
  const [y,m,d]=date.split('-').map(Number);return `${y===now.getFullYear()?'':y+'年'}${m}月${d}日`;
}

// Undated items and unfinished overdue items stay actionable in today's agenda.
export function agendaNotes(notes,view='today',now=new Date()) {
  const today=localDate(now);
  return notes.filter(n=>view==='all'||(view==='upcoming'?n.date>today:!n.date||n.date===today||(n.date<today&&!n.done)));
}
export function mergeVisibleOrder(allIds,visibleIds) {
  const set=new Set(visibleIds);
  if(set.size!==visibleIds.length||visibleIds.some(id=>!allIds.includes(id)))throw new Error('待办顺序已变化');
  let i=0;return allIds.map(id=>set.has(id)?visibleIds[i++]:id);
}
export function quickDates(now=new Date()) {
  const at=offset=>new Date(now.getFullYear(),now.getMonth(),now.getDate()+offset);
  const result=['今天','明天','后天'].map((label,i)=>({label,date:localDate(at(i))}));
  for(let offset=3,count=0;count<5;offset++){
    const day=at(offset);if(day.getDay()===0||day.getDay()===6)continue;
    result.push({label:`周${'日一二三四五六'[day.getDay()]} ${day.getMonth()+1}/${day.getDate()}`,date:localDate(day)});count++;
  }
  return result;
}

// Only a widget touching the edge uses the dock shape; dragging away is a full sphere.
export function dockAppearance(anchor,monitors) {
  const r=compactRect(anchor,monitors),a=areaOf(monitorFor({x:r.x+1,y:r.y+1},monitors));
  const edges=[['left',r.x-a.x],['right',a.x+a.width-r.x-r.width],['top',r.y-a.y],['bottom',a.y+a.height-r.y-r.height]].sort((a,b)=>a[1]-b[1]);
  return edges[0][1]<=0?{edge:edges[0][0],amount:1}:{edge:'free',amount:0};
}

export function draggedExpanded(point,grab,monitors) {
  const a=areaOf(monitorFor(point,monitors)),width=Math.min(a.width,Math.round(380*a.scale)),height=Math.min(a.height,Math.round(460*a.scale));
  const x=Math.round(Math.max(a.x,Math.min(point.x-grab.u*width,a.x+a.width-width)));
  const y=Math.round(Math.max(a.y,Math.min(point.y-grab.v*height,a.y+a.height-height)));
  const size=Math.round(COMPACT*a.scale);
  return {rect:{x,y,width,height,fx:grab.fx,fy:grab.fy},anchor:{x:Math.round(x+grab.fx*(width-size)),y:Math.round(y+grab.fy*(height-size))}};
}
