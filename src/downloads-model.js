export function bytes(n){n=Number(n)||0;const units=['B','KiB','MiB','GiB','TiB'];let i=0;while(n>=1024&&i<units.length-1){n/=1024;i++;}return `${n.toFixed(i?1:0)} ${units[i]}`;}
export function taskView(t){
 const p=t.progress||{},total=Number(t.size)||0,downloaded=Number(p.downloaded)||0,speed=Number(p.speed)||0;
 const status={ready:'等待中',running:'下载中',pause:'已暂停',wait:'排队中',done:'已完成',error:'下载失败'}[t.status]||String(t.status||'准备中');
 const done=t.status==='done',running=['running','ready','wait'].includes(t.status);
 const percent=done?100:total>0?Math.max(0,Math.min(100,downloaded/total*100)):0;
 const eta=speed>0&&total>downloaded?` · 剩余约 ${Math.ceil((total-downloaded)/speed)} 秒`:'';
 return {status,done,percent,canPause:running,canContinue:['pause','error'].includes(t.status),detail:`${bytes(downloaded)} / ${total>0?bytes(total):'大小未知'}${running?' · '+bytes(speed)+'/s'+eta:''}`};
}
