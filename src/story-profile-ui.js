const traits={social:['社交倾向','偏好独处 → 偏好社交'],direct:['表达直接','委婉 → 直接'],order:['自律条理','随性 → 有条理'],warmth:['温和体贴','疏淡 → 温和'],curiosity:['好奇开放','偏好熟悉 → 喜欢探索'],steady:['情绪稳健','容易波动 → 稳定']};
const speech={short:'短句',formal:'正式',casual:'随意',dry_humor:'冷幽默',playful:'俏皮',few_questions:'少反问',blunt:'直白',indirect:'委婉',reserved:'克制',expressive:'外露'};
const moods={tension:'紧张',pleasure:'愉快',energy:'精力'},relations={trust:'信任',familiarity:'熟悉',guard:'戒备'};
export function renderCharacterProfile(profile){const node=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!=null)n.textContent=text;return n;};const root=node('section','character-profile');root.append(node('strong','','紧凑人设'));if(!profile){root.append(node('p','profile-hint','尚未整理；会根据已有创作资料逐步形成，未知维度保持未定。'));return root;}
root.append(node('p','profile-hint','0–100 表示演绎倾向，不是心理测评；未定表示资料不足。'));
for(const [key,[label,hint]]of Object.entries(traits)){const row=node('div','profile-axis');row.title=hint;row.append(node('span','',label));const value=profile.traits?.[key];if(Number.isInteger(value)){const meter=node('meter');meter.min=0;meter.max=100;meter.value=value;meter.setAttribute('aria-label',label);row.append(meter,node('span','profile-value',String(value)));}else row.append(node('span','profile-unknown','未定'));root.append(row);}
root.append(node('p','profile-speech','说话习惯：'+((profile.speech||[]).map(t=>speech[t]||t).join('、')||'未定')));
for(const text of Object.values(profile.anchors||{}))root.append(node('p','profile-anchor',text));
const mood=Object.entries(profile.mood||{}).map(([k,v])=>`${moods[k]||k} ${v}`).join(' · ');if(mood)root.append(node('p','profile-mood','此刻：'+mood));
for(const [name,r]of Object.entries(profile.relations||{}))root.append(node('p','profile-relation',name+'：'+Object.entries(r).map(([k,v])=>`${relations[k]||k} ${v}`).join(' · ')));
if(profile.needs_review)root.append(node('p','profile-hint','稳定人设待整理；已保存的特点、情绪和关系会保留。'));return root;}

export function describeProfileChanges(before,after){const changes=[];for(const [key,[label]]of Object.entries(traits)){const a=before?.traits?.[key],b=after?.traits?.[key];if(a!==b)changes.push(`${label}：${a??'未定'} → ${b??'未定'}`);}const old=new Set(before?.speech||[]),next=new Set(after?.speech||[]);for(const tag of next)if(!old.has(tag))changes.push('习惯增加：'+(speech[tag]||tag));for(const tag of old)if(!next.has(tag))changes.push('习惯移除：'+(speech[tag]||tag));return changes;}
