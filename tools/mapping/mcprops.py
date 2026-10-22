import json, os, sys
d = sys.argv[1]; out = {}
for fn in sorted(os.listdir(d)):
    j = json.load(open(os.path.join(d, fn)))
    props = {}
    def add(k, v):
        for x in str(v).split('|'): props.setdefault(k, set()).add(x)
    for key in j.get('variants', {}):
        for kv in filter(None, key.split(',')):
            k, v = kv.split('='); add(k, v)
    def walk(w):
        if not w: return
        for k, v in w.items():
            if k in ('OR', 'AND'):
                for x in v: walk(x)
            else: add(k, v)
    for part in j.get('multipart', []): walk(part.get('when'))
    out['minecraft:' + fn[:-5]] = {k: sorted(v) for k, v in props.items()}
json.dump(out, open(sys.argv[2], 'w'), indent=0)
print(len(out))
