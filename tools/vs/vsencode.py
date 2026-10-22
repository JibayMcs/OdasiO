#!/usr/bin/env python3
"""Reference encoder for VS 1.22.7 chunk / mapchunk blobs (mirrors what the Rust writer must do)."""
import os, struct, subprocess, tempfile

def zstd_c(raw, level=3):
    # File-based so the frame header carries Frame_Content_Size (REQUIRED by VS reader).
    with tempfile.TemporaryDirectory() as d:
        a = os.path.join(d, "a"); open(a, "wb").write(raw)
        subprocess.run(["zstd", "-q", "-f", "-%d" % level, "--no-check", a, "-o", a + ".zst"], check=True)
        return open(a + ".zst", "rb").read()

# ---------- protobuf primitives ----------
def uvarint(n):
    n &= (1 << 64) - 1
    out = bytearray()
    while True:
        b = n & 0x7F; n >>= 7
        if n: out.append(b | 0x80)
        else: out.append(b); return bytes(out)

def i32varint(n):  # protobuf int32: negative -> 10-byte two's complement (protobuf-net accepts)
    return uvarint(n if n >= 0 else n + (1 << 64))

def tag(f, wt): return uvarint((f << 3) | wt)
def f_varint(f, v): return tag(f, 0) + i32varint(v)
def f_bytes(f, b): return tag(f, 2) + uvarint(len(b)) + b

# ---------- palette layer ----------
def bits_for(count):
    # CalcBitsize: ceil(log2(count)), min 1 for count>=2
    n, b = count - 1, 1
    while (n := n >> 1) > 0: b += 1
    return b

def planes_bytes(indices, nbits):
    """indices: 32768 palette indices -> nbits*1024 little-endian int32 words (bit-planar)."""
    words = [0] * (nbits * 1024)
    for i, v in enumerate(indices):
        if v == 0: continue
        w = i >> 5; bit = i & 31
        for k in range(nbits):
            if (v >> k) & 1: words[k * 1024 + w] |= 1 << bit
    return struct.pack("<%dI" % len(words), *words)

def build_palette(values):
    pal = [0]  # palette[0] is always 0 (air / no fluid / no light), as the game does
    idx = {0: 0}
    out = []
    for v in values:
        j = idx.get(v)
        if j is None:
            j = idx[v] = len(pal); pal.append(v)
        out.append(j)
    return pal, out

def encode_block_layer(values):
    """blocks (field 1) or fluids (field 16): 'combined' format."""
    if all(v == 0 for v in values):
        return b"\0\0\0\0"            # == ChunkDataLayer.emptyCompressed -> layer null
    pal, idx = build_palette(values)
    nbits = bits_for(len(pal))
    data = zstd_c(planes_bytes(idx, nbits))
    palraw = struct.pack("<%di" % len(pal), *pal)
    if len(pal) > 18:
        pz = zstd_c(palraw)
        return struct.pack("<i", len(pz)) + pz + data
    return struct.pack("<i", -len(palraw)) + palraw + data

def encode_light_layer(values):
    """returns (lightCompressed field2, lightPaletteCompressed field3)."""
    if values is None:
        return b"", b""
    pal, idx = build_palette(values)
    nbits = bits_for(len(pal)) if len(pal) > 1 else 1
    # palette array length must be 2^nbits for the reader's bitsize derivation; pad with zeros
    padded = pal + [0] * ((1 << nbits) - len(pal))
    # NOTE: the game writes paletteCount ints (not padded); reader rounds up to pow2. Either works.
    return zstd_c(planes_bytes(idx, nbits)), zstd_c(struct.pack("<%di" % len(pal), *pal))

def encode_chunk(blocks, fluids=None, light=None, version="1.22.7"):
    fluids = fluids or [0] * 32768
    b = encode_block_layer(blocks)
    l, lp = encode_light_layer(light)
    out = f_bytes(1, b) + f_bytes(2, l) + f_bytes(3, lp)
    out += f_bytes(12, version.encode())
    if all(v == 0 for v in blocks) and all(v == 0 for v in fluids):
        out += f_varint(13, 1)
    out += f_varint(15, 2)
    out += f_bytes(16, encode_block_layer(fluids))
    return out

def encode_mapchunk(rain, terrain, ymax, currentpass, worldgenversion=3, toprock=None, light_updates=()):
    out = b""
    out += b"".join(f_varint(3, h) if h else tag(3, 0) + b"\0" for h in rain)   # repeated, unpacked
    out += f_varint(4, currentpass)
    out += b"".join(tag(7, 0) + uvarint(h) for h in terrain)
    for (x, y, z, bid) in light_updates:  # field 17 List<Vec4i>, Vec4i = {1:X,2:Y,3:Z,4:W}
        out += f_bytes(17, f_varint(1, x) + f_varint(2, y) + f_varint(3, z) + f_varint(4, bid))
    out += f_varint(10, ymax)
    out += b"".join(tag(13, 0) + i32varint(r) for r in (toprock or [0] * 1024))
    out += f_varint(16, worldgenversion)
    return out

def chunk_index(cx, cy, cz, dim=0):
    v = (cx & 0xFFFFFFFF) | (cz << 27) | (cy << 54)
    if dim:
        v |= (dim & 31) << 22
        v |= (dim & 992) << 44
    v &= (1 << 64) - 1
    return v - (1 << 64) if v >= 1 << 63 else v   # SQLite INTEGER is signed
