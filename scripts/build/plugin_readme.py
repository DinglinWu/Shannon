"""README contract shared by both plugin packagers; Rust validates again on install."""
FIELDS=('id','name','author','version','description')
def validate(text, metadata):
    if len(text.encode('utf-8'))>65536 or '\0' in text:raise ValueError('README.md exceeds 64 KiB or contains NUL')
    lines=text.lstrip('\ufeff').splitlines()
    if not lines or lines[0]!='---':raise ValueError('README.md must start with --- metadata')
    fields={};end=None
    for i,line in enumerate(lines[1:],1):
        if line=='---':end=i;break
        key,separator,value=line.partition(':');key=key.strip();value=value.strip()
        if not separator or key not in FIELDS or key in fields:raise ValueError('README.md has invalid or duplicate metadata')
        fields[key]=value
    if end is None or not '\n'.join(lines[end+1:]).strip():raise ValueError('README.md requires a body')
    for key in FIELDS:
        if not metadata.get(key) or fields.get(key)!=metadata[key]:raise ValueError(f'README.md {key} must match plugin manifest')
    return text
