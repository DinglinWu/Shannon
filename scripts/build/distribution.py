"""The two publication sites share identical signed release assets."""
RELEASE_BASES = {
    'gitee': 'https://gitee.com/wu-dinglin/shannon/releases/download',
    'github': 'https://github.com/DinglinWu/Shannon/releases/download',
}
MANIFESTS = {'gitee': 'latest.json', 'github': 'latest-github.json'}
BRANCHES = {'gitee': 'master', 'github': 'main'}
