import catalog from './wechat-avatar-catalog.json' with {type:'json'};
export {catalog};
export function avatarId(id,name='我'){
 if(isCustomAvatar(id))return id;
 if(catalog.some(item=>item.id===id))return id;
 let hash=2166136261;for(const byte of new TextEncoder().encode(name))hash=Math.imul(hash^byte,16777619)>>>0;
 return catalog[hash%catalog.length].id;
}
export function isCustomAvatar(id){return typeof id==='string'&&/^custom-[0-9a-f]{64}$/.test(id);}
export function avatarPath(id,name){return `/avatars/${avatarId(isCustomAvatar(id)?'':id,name)}.svg`;}
