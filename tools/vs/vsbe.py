#!/usr/bin/env python3
"""Vintage Story 1.22.7 block-entity helpers: TreeAttribute (de)serializer + BE blob builders.
Reference implementation for docs/vs-block-entities.md (mirrors TreeAttribute.ToBytes/FromBytes, API 33979).

A tree is a Python list of (key, type, value) triples (order preserved) so that the byte output is
deterministic.  Types: 'int','long','double','float','str','tree','itemstack','bytes','bool',
'strarr','intarr','floatarr','doublearr','treearr','longarr','boolarr'.
"""
import struct

T_ID = {'int': 1, 'long': 2, 'double': 3, 'float': 4, 'str': 5, 'tree': 6, 'itemstack': 7, 'bytes': 8,
        'bool': 9, 'strarr': 10, 'intarr': 11, 'floatarr': 12, 'doublearr': 13, 'treearr': 14,
        'longarr': 15, 'boolarr': 16}
ID_T = {v: k for k, v in T_ID.items()}


# ---------- .NET BinaryWriter primitives ----------
def w7(n):  # Write7BitEncodedInt (string length prefix)
    out = bytearray()
    while n >= 0x80:
        out.append((n & 0x7F) | 0x80); n >>= 7
    out.append(n)
    return bytes(out)

def wstr(s):
    b = s.encode('utf-8')
    return w7(len(b)) + b

def r7(b, i):
    r = s = 0
    while True:
        c = b[i]; i += 1
        r |= (c & 0x7F) << s; s += 7
        if c < 0x80: return r, i

def rstr(b, i):
    n, i = r7(b, i)
    return b[i:i + n].decode('utf-8'), i + n


# ---------- TreeAttribute ----------
def enc_value(t, v):
    if t == 'int': return struct.pack('<i', v)
    if t == 'long': return struct.pack('<q', v)
    if t == 'double': return struct.pack('<d', v)
    if t == 'float': return struct.pack('<f', v)
    if t == 'str': return wstr(v or '')
    if t == 'tree': return enc_tree(v)
    if t == 'itemstack':
        if v is None: return b'\x01'                      # bool isNull = true
        cls, iid, size, attrs = v                         # cls: 0 block, 1 item
        return b'\x00' + struct.pack('<iii', cls, iid, size) + enc_tree(attrs or [])
    if t == 'bytes': return struct.pack('<H', len(v)) + bytes(v)   # ushort length!
    if t == 'bool': return b'\x01' if v else b'\x00'
    if t == 'strarr': return struct.pack('<i', len(v)) + b''.join(wstr(s) for s in v)
    if t == 'intarr': return struct.pack('<i%di' % len(v), len(v), *v)
    if t == 'floatarr': return struct.pack('<i%df' % len(v), len(v), *v)
    if t == 'doublearr': return struct.pack('<i%dd' % len(v), len(v), *v)
    if t == 'longarr': return struct.pack('<i%dq' % len(v), len(v), *v)
    if t == 'boolarr': return struct.pack('<i', len(v)) + bytes(1 if x else 0 for x in v)
    if t == 'treearr': return struct.pack('<i', len(v)) + b''.join(enc_tree(x) for x in v)
    raise ValueError(t)

def enc_tree(items):
    out = bytearray()
    for k, t, v in items:
        out.append(T_ID[t]); out += wstr(k); out += enc_value(t, v)
    out.append(0)                                         # TerminateWrite
    return bytes(out)

def dec_value(t, b, i):
    if t == 'int': return struct.unpack_from('<i', b, i)[0], i + 4
    if t == 'long': return struct.unpack_from('<q', b, i)[0], i + 8
    if t == 'double': return struct.unpack_from('<d', b, i)[0], i + 8
    if t == 'float': return struct.unpack_from('<f', b, i)[0], i + 4
    if t == 'str': return rstr(b, i)
    if t == 'tree': return dec_tree(b, i)
    if t == 'itemstack':
        if b[i]: return None, i + 1
        cls, iid, size = struct.unpack_from('<iii', b, i + 1)
        attrs, i = dec_tree(b, i + 13)
        return (cls, iid, size, attrs), i
    if t == 'bytes':
        n = struct.unpack_from('<h', b, i)[0]; return b[i + 2:i + 2 + n], i + 2 + n
    if t == 'bool': return bool(b[i]), i + 1
    n = struct.unpack_from('<i', b, i)[0]; i += 4
    if t == 'strarr':
        out = []
        for _ in range(n):
            s, i = rstr(b, i); out.append(s)
        return out, i
    fmt = {'intarr': 'i', 'floatarr': 'f', 'doublearr': 'd', 'longarr': 'q'}.get(t)
    if fmt:
        sz = struct.calcsize(fmt)
        return list(struct.unpack_from('<%d%s' % (n, fmt), b, i)), i + n * sz
    if t == 'boolarr': return [bool(x) for x in b[i:i + n]], i + n
    if t == 'treearr':
        out = []
        for _ in range(n):
            x, i = dec_tree(b, i); out.append(x)
        return out, i
    raise ValueError(t)

def dec_tree(b, i=0):
    items = []
    while True:
        tid = b[i]; i += 1
        if tid == 0: return items, i
        k, i = rstr(b, i)
        t = ID_T[tid]
        v, i = dec_value(t, b, i)
        items.append((k, t, v))


# ---------- block entity blob (ServerChunk field 8 element) ----------
def be_blob(classname, x, y, z, block_code, extra=()):
    """block_code: short code without 'game:' (e.g. 'chest-north')."""
    tree = [('posx', 'int', x), ('posy', 'int', y), ('posz', 'int', z)]
    if block_code is not None:
        tree.append(('blockCode', 'str', block_code))
    tree += list(extra)
    return wstr(classname) + enc_tree(tree)

def parse_be_blob(b):
    cls, i = rstr(b, 0)
    tree, i = dec_tree(b, i)
    assert i == len(b), (i, len(b))
    return cls, tree

def fmt_tree(items, ind='  '):
    lines = []
    for k, t, v in items:
        if t == 'tree':
            lines.append(f'{ind}{k} (tree):'); lines += fmt_tree(v, ind + '  ')
        elif t == 'itemstack' and v is not None:
            lines.append(f'{ind}{k} (itemstack): class={v[0]} id={v[1]} size={v[2]} attrs={v[3]}')
        else:
            lines.append(f'{ind}{k} ({t}) = {v!r}')
    return lines
