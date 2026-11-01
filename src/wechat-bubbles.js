export function chatBubbles(text){
 if(text.includes('```')||text.includes('~~~'))return [text];
 const parts=text.split(/\n[ \t]*\n+/).map(s=>s.trim()).filter(Boolean);
 if(parts.length<2)return [text];
 return parts.length<=3?parts:[parts[0],parts[1],parts.slice(2).join('\n\n')];
}
