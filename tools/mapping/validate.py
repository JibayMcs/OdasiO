import sys, tomli, collections
codes = set(l.strip() for l in open(sys.argv[1]) if l.strip())
m = tomli.load(open(sys.argv[2], 'rb'))
bad = []
for r in m['settings']['safe_rocks']:
    for b in m['block'] + [{'mc': 'default', 'vs': v} for v in m['default'].values()]:
        if b['vs'].replace('{rock}', r) not in codes: bad.append((r, b['mc'], b['vs']))
print('blocks', len(m['block']), 'bad', len(bad), bad[:5])
ids = collections.defaultdict(list)
for b in m['block']: ids[b['mc']].append(b)
# rules after an unconditional rule are unreachable
for k, es in ids.items():
    for i, e in enumerate(es[:-1]):
        if 'when' not in e: print('unreachable after', k)
print('ids', len(ids), 'all-approx', sum(all(e.get('approx') for e in es) for es in ids.values()),
      'exact', sum(not any(e.get('approx') for e in es) for es in ids.values()))
