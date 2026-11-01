"""Publish metadata only after the caller has verified the signed installer."""
import datetime
import hashlib
import json
import os
from pathlib import Path
from urllib.parse import quote

def write_release_metadata(root, output, version, installer, signature):
    notes=(root/'docs/release/RELEASE_NOTES.md').read_text(encoding='utf-8')
    signed=signature.read_text(encoding='utf-8').strip()
    if not installer.is_file() or not signed:raise ValueError('安装包或更新签名缺失，不生成发行 JSON')
    date=datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    latest={'version':version,'notes':notes,'pub_date':date,'platforms':{'windows-x86_64':{'signature':signed,'url':f'https://gitee.com/wu-dinglin/shannon/releases/download/v{version}/{quote(installer.name)}'}}}
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
    release={'schema_version':1,'tag_name':f'v{version}','name':f'Shannon {version}','body':notes,'pub_date':date,'assets':assets,'update_manifest':{'path':'release/app/latest.json','publish_to':'master/latest.json'},'publish_order':['先上传 assets 中的发行附件','确认附件可下载后，再推送根目录 latest.json']}
    # No credentials, private key paths, or API tokens belong in these JSON files.
    output.mkdir(parents=True,exist_ok=True)
    latest_raw=(json.dumps(latest,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
    release_raw=(json.dumps(release,ensure_ascii=False,indent=2)+'\n').encode('utf-8')
    for path,raw in [(output/'latest.json',latest_raw),(output/'release.json',release_raw),(root/'latest.json',latest_raw)]:
        temp=path.with_name(path.name+'.next');temp.write_bytes(raw);os.replace(temp,path)
    return latest,release
