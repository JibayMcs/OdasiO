#!/usr/bin/env python3
"""Minimal VS .vcdbs inspector (protobuf + zstd via CLI)."""
import sqlite3, subprocess, struct, sys, collections

def zstd_d(b):
    return subprocess.run(["zstd", "-d", "-c", "-q"], input=b, stdout=subprocess.PIPE, check=True).stdout

def varint(b, i):
    r = s = 0
    while True:
        c = b[i]; i += 1
        r |= (c & 0x7F) << s; s += 7
        if c < 0x80: return r, i

def pb(b):
    """yield (field, wiretype, value) ; value = int for 0/1/5, bytes for 2"""
    i = 0
    out = []
    while i < len(b):
        k, i = varint(b, i)
        f, wt = k >> 3, k & 7
        if wt == 0:
            v, i = varint(b, i)
        elif wt == 2:
            l, i = varint(b, i); v = b[i:i+l]; i += l
        elif wt == 5:
            v = b[i:i+4]; i += 4
        elif wt == 1:
            v = b[i:i+8]; i += 8
        else:
            raise ValueError("wt %d at %d" % (wt, i))
        out.append((f, wt, v))
    return out

def summarize(msg):
    c = collections.OrderedDict()
    for f, wt, v in msg:
        c.setdefault((f, wt), []).append(v)
    for (f, wt), vs in c.items():
        if wt == 2:
            print(f"  field {f} LEN x{len(vs)} first_len={len(vs[0])} head={vs[0][:16].hex()}")
        else:
            print(f"  field {f} wt{wt} x{len(vs)} first={vs[0] if wt==0 else vs[0].hex()} {'...' if len(vs)>1 else ''}")

def decode_pos(p):
    p &= (1 << 64) - 1
    x = p & 0x3FFFFF; z = (p >> 27) & 0x3FFFFF; y = (p >> 54) & 0x1FF
    dim = ((p >> 22) & 31) + ((p >> 44) & 992)
    return x, y, z, dim

def combined(b):
    """blocks/fluids layer: returns (palette, planes)"""
    h = struct.unpack_from("<i", b, 0)[0]
    if h == 0: return None, None
    if h < 0:
        n = -h // 4
        pal = list(struct.unpack_from("<%di" % n, b, 4)); off = 4 - h
    else:
        raw = zstd_d(b[4:4+h]); n = len(raw)//4
        pal = list(struct.unpack("<%di" % n, raw)); off = 4 + h
    if n <= 1: return pal, None
    raw = zstd_d(b[off:])
    return pal, raw

def light(data, pal):
    if not data or not pal: return None, None
    p = zstd_d(pal); d = zstd_d(data)
    return list(struct.unpack("<%di" % (len(p)//4), p)), len(d)

def get_block(planes, pal, idx):
    nbits = (len(planes)//4096)
    v = 0
    for k in range(nbits):
        w = struct.unpack_from("<i", planes, (k*1024 + idx//32)*4)[0]
        v |= ((w >> (idx & 31)) & 1) << k
    return pal[v]

if __name__ == "__main__":
    db = sqlite3.connect(sys.argv[1])
    what = sys.argv[2]
    if what == "chunk":
        for pos, data in db.execute("select position,data from chunk order by position limit ?", (int(sys.argv[3]) if len(sys.argv) > 3 else 3,)):
            print("pos", pos, decode_pos(pos), "len", len(data))
            m = pb(data); summarize(m)
            d = {f: v for f, wt, v in m}
            pal, planes = combined(d.get(1, b"\0\0\0\0"))
            print("  blocks palette", pal, "planes bytes", None if planes is None else len(planes))
            if planes:
                col = [get_block(planes, pal, (y*32+0)*32+0) for y in range(32)]
                print("  column x0z0 y0..31:", col)
            print("  fluids", combined(d.get(16, b"\0\0\0\0"))[0])
            lp, ld = light(d.get(2), d.get(3))
            print("  light palette", lp, "planes bytes", ld)
    elif what == "mapchunk":
        for pos, data in db.execute("select position,data from mapchunk order by position limit 1"):
            print("pos", pos, decode_pos(pos), "len", len(data))
            m = pb(data); summarize(m)
            for f, wt, v in m:
                if f in (4, 10, 16): print("  f", f, "=", v)
    elif what == "gamedata":
        (data,) = db.execute("select data from gamedata").fetchone()
        m = pb(data); summarize(m)
        for f, wt, v in m:
            if wt == 0: print("  int f", f, v)
            elif wt == 2 and f in (13, 17, 18, 21, 22, 29, 30, 32, 34): print("  str f", f, v.decode())
            elif wt == 5: print("  f32 f", f, struct.unpack("<f", v)[0])
            elif f == 11:
                kv = {ff: vv for ff, _, vv in pb(v)}
                print("  moddata key", kv.get(1, b"").decode(), "len", len(kv.get(2, b"")))
            elif f == 40:
                print("  DefaultSpawn", pb(v))
