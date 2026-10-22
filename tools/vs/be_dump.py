#!/usr/bin/env python3
"""Dump the block entities (ServerChunk field 8) of a .vcdbs, optionally restricted to one column.
Usage: be_dump.py <world.vcdbs> [chunkX chunkZ]
"""
import sys, os, sqlite3
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vsparse as p
import vsbe as b

db = sqlite3.connect(sys.argv[1])
want = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) > 3 else None
for pos, data in db.execute("select position,data from chunk order by position"):
    x, y, z, dim = p.decode_pos(pos)
    if want and (x, z) != want: continue
    msg = p.pb(data)
    bes = [v for f, wt, v in msg if f == 8]
    cnt = [v for f, wt, v in msg if f == 7]
    if not bes: continue
    print(f"chunk {x},{y},{z} BlockEntitiesCount={cnt} entries={len(bes)}")
    for blob in bes:
        cls, tree = b.parse_be_blob(blob)
        print(f" - {cls}  ({len(blob)} bytes)")
        print("\n".join(b.fmt_tree(tree, '     ')))
