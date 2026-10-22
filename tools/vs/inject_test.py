#!/usr/bin/env python3
"""Inject hand-encoded test columns into a copy of the flat test world.
Column A (16000,16000): light empty, mapchunk currentpass=3 (Vegetation) -> game should light it.
Column B (16001,16000): our own naive sunlight, currentpass=6 (Done) -> game should load as-is.
"""
import sys, sqlite3, json
sys.path.insert(0, sys.argv[2])
import vsencode as e

db = sqlite3.connect(sys.argv[1])
GRANITE, GLASS, TORCH, WATER = 4605, 342, 9665, 7416
ROCKS = list(range(4599, 4623))  # 24 distinct rock ids -> forces zstd palette path (>18)
SUN = 22
H = 256 // 32  # ChunkMapSizeY

def idx(x, y, z): return (y * 32 + z) * 32 + x

def build_column(cx, cz):
    cols = [[0] * 32768 for _ in range(H)]
    fl = [[0] * 32768 for _ in range(H)]
    def setb(x, y, z, v): cols[y // 32][idx(x, y % 32, z)] = v
    for x in range(32):
        for z in range(32):
            for y in range(0, 3): setb(x, y, z, GRANITE)
            setb(x, 3, z, ROCKS[(x + z) % len(ROCKS)])
    for y in range(4, 41): setb(5, y, 5, GRANITE)      # pillar crossing the cy=0/1 boundary
    setb(5, 41, 5, TORCH)
    setb(20, 4, 20, GLASS)
    fl[0][idx(10, 4, 10)] = WATER                        # fluid layer test
    rain = [0] * 1024
    for x in range(32):
        for z in range(32):
            top = 3
            if (x, z) == (5, 5): top = 41
            if (x, z) == (20, 20): top = 4
            if (x, z) == (10, 10): top = 4
            rain[z * 32 + x] = top
    return cols, fl, rain

def naive_light(cols, rain):
    out = []
    for cy in range(H):
        L = [0] * 32768
        for y in range(32):
            for z in range(32):
                for x in range(32):
                    if cy * 32 + y > rain[z * 32 + x]: L[idx(x, y, z)] = SUN
        out.append(L)
    return out

for (cx, cz, mode) in [(16000, 16000, "A"), (16001, 16000, "B")]:
    cols, fl, rain = build_column(cx, cz)
    light = naive_light(cols, rain) if mode == "B" else [None] * H
    for cy in range(H):
        blob = e.encode_chunk(cols[cy], fl[cy], light[cy])
        db.execute("insert or replace into chunk(position,data) values(?,?)", (e.chunk_index(cx, cy, cz), blob))
    lu = [(cx * 32 + 5, 41, cz * 32 + 5, TORCH)] if mode == "A" else []
    mc = e.encode_mapchunk(rain, rain, 41, 3 if mode == "A" else 6, 3, None, lu)
    db.execute("insert or replace into mapchunk(position,data) values(?,?)", (e.chunk_index(cx, 0, cz), mc))
    open(sys.argv[3] + "/col%s_cy0.bin" % mode, "wb").write(e.encode_chunk(cols[0], fl[0], light[0]))
db.commit()
print("ok")
