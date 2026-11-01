"""Publish metadata only after the caller has verified the signed installer."""
import datetime
import hashlib
import json
import os
import shutil
from pathlib import Path
from urllib.parse import quote
from distribution import RELEASE_BASES, MANIFESTS, BRANCHES

def write_release_metadata(root, output, version, installer, signature):
    notes=(root/'docs/release/RELEASE_NOTES.md').read_text(encoding='utf-8')
    signed=signature.read_text(encoding='utf-8').strip()
    if not installer.is_file() or not signed:raise ValueError('安装包或更新签名缺失，不生成发行 JSON')
    date=datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    def manifest(site):
        return {'version':version,'notes':notes,'pub_date':date,'platforms':{'windows-x86_64':{'signature':signed,'url':f'{RELEASE_BASES[site]}/v{version}/{quote(installer.name)}'}}}
    latest=manifest('gitee')
    assets=[]
    def asset(path,kind,expected=None):
        raw=path.read_bytes();digest=hashlib.sha256(raw).hexdigest()
        if expected and digest!=expected:raise ValueError(f'待发布插件包哈希不匹配：{path.name}')
        assets.append({'name':path.name,'path':path.relative_to(root).as_posix(),'kind':kind,'bytes':len(raw),'sha256':digest})
    asset(installer,'installer');asset(signature,'updater_signature')
    catalog_path=root/'src-tauri/official-plugins.json'
    if catalog_path.exists():
        for plugin in json.loads(catalog_path.read_text(encoding='utf-8'))['plugins']:
            spec=plugin['runtime'];name=spec['url'].rsplit('/',1)[1]
            if Path(name).name!=name:raise ValueError('插件附件名无效')
            asset(root/'release/plugins'/name,'official_plugin',spec['sha256'])
    release={'schema_version':1,'tag_name':f'v{version}','name':f'Shannon {version}','body':notes,'pub_date':date,'assets':assets,'update_manifest':{'path':(output/'latest.json').relative_to(root).as_posix(),'publish_to':'master/latest.json'},'publish_order':['先上传 assets 中的发行附件','确认附件可下载后，再推送根目录对应的更新 JSON']}
    # No credentials, private key paths, or API tokens belong in these JSON files.
    output.mkdir(parents=True,exist_ok=True)
    latest_raw=(json.dumps(latest,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
    release_raw=(json.dumps(release,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
    writes=[(output/'latest.json',latest_raw),(output/'release.json',release_raw)]
    for site in RELEASE_BASES:
        folder=root/'release'/site
        folder.mkdir(parents=True,exist_ok=True)
        site_assets=[]
        for item in assets:
            source=root/item['path'];target=folder/item['name']
            shutil.copy2(source,target)
            site_assets.append({**item,'path':target.relative_to(root).as_posix()})
        site_latest=manifest(site)
        site_release={**release,'site':site,'assets':site_assets,'update_manifest':{
            'path':(folder/'latest.json').relative_to(root).as_posix(),
            'publish_to':f'{BRANCHES[site]}/{MANIFESTS[site]}'}}
        writes.extend([(folder/'latest.json',(json.dumps(site_latest,ensure_ascii=False,indent=2)+'\n').encode('utf-8')),
            (folder/'release.json',(json.dumps(site_release,ensure_ascii=False,indent=2)+'\n').encode('utf-8'))])
    # Root discovery manifests are written last, after every attachment was validated/copied.
    writes.extend([(root/MANIFESTS[site],(json.dumps(manifest(site),ensure_ascii=False,indent=2)+'\n').encode('utf-8')) for site in RELEASE_BASES])
    for path,raw in writes:
        temp=path.with_name(path.name+'.next');temp.write_bytes(raw);os.replace(temp,path)
    return latest,release
