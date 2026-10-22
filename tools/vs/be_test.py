#!/usr/bin/env python3
"""Block-entity injection experiment (docs/vs-block-entities.md, section 6).
Usage: be_test.py <world.vcdbs (copy of the flat test world)> <blockids.json>
Column A (16000,16000): currentpass=3 => the server lights it and RE-SAVES it (round-trip of our BEs).
Column B (16001,16000): currentpass=6, naive sunlight => loaded as-is.
Prints the list of injected BEs; decode the result with be_dump.py.
"""
import sys, os, json, math, sqlite3
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vsencode as e
import vsbe as b

db = sqlite3.connect(sys.argv[1])
ids = {v: int(k) for k, v in json.load(open(sys.argv[2])).items()}
H = 256 // 32
SUN = 22
GRANITE = ids['rock-granite']

def idx(x, y, z): return (y * 32 + z) * 32 + x

def column(cx, cz, place, bes, light):
    cols = [[0] * 32768 for _ in range(H)]
    for x in range(32):
        for z in range(32):
            for y in range(4): cols[0][idx(x, y, z)] = GRANITE
    rain = [3] * 1024
    for (x, y, z), code in place.items():
        cols[y // 32][idx(x, y % 32, z)] = ids[code]
        rain[z * 32 + x] = max(rain[z * 32 + x], y)
    by_chunk = {}
    for (x, y, z), blob in bes:
        by_chunk.setdefault(y // 32, []).append(blob)
    for cy in range(H):
        L = None
        if light:
            L = [SUN if cy * 32 + y > rain[z * 32 + x] else 0
                 for y in range(32) for z in range(32) for x in range(32)]
        blob = e.encode_chunk(cols[cy], None, L)
        if cy in by_chunk:   # fields 7 + 8
            blob += e.f_varint(7, len(by_chunk[cy]))
            blob += b''.join(e.f_bytes(8, x) for x in by_chunk[cy])
        db.execute("insert or replace into chunk(position,data) values(?,?)", (e.chunk_index(cx, cy, cz), blob))
    return rain

def mapchunk(cx, cz, rain, cpass, new_be=()):
    mc = e.encode_mapchunk(rain, rain, max(rain), cpass, 3)
    for (x, y, z) in new_be:   # field 9 NewBlockEntities (BlockPos {1:X,2:Y,3:Z})
        mc += e.f_bytes(9, e.f_varint(1, x) + e.f_varint(2, y) + e.f_varint(3, z))
    db.execute("insert or replace into mapchunk(position,data) values(?,?)", (e.chunk_index(cx, 0, cz), mc))

def build(cx, cz, cpass):
    X0, Z0 = cx * 32, cz * 32
    place, bes, newbe = {}, [], []
    def put(lx, y, lz, code, cls=None, extra=(), blockcode='same'):
        if code: place[(lx, y, lz)] = code
        if cls:
            bc = code if blockcode == 'same' else blockcode
            bes.append(((lx, y, lz), b.be_blob(cls, X0 + lx, y, Z0 + lz, bc, extra)))
            print(f"  BE {cls:22s} {code or '-':28s} at {X0+lx},{y},{Z0+lz} extra={[k for k,_,_ in extra]}")
    inv = lambda slots, q: ('inventory', 'tree', [('qslots', 'int', q), ('slots', 'tree', slots)])
    # 1 chest, full (forBlockCode, type, meshAngle, inventory with 5 granite in slot 0)
    put(2, 4, 2, 'chest-north', 'GenericTypedContainer', [
        ('forBlockCode', 'str', 'chest-north'), ('type', 'str', 'normal-generic'), ('meshAngle', 'float', math.pi / 2),
        inv([('0', 'itemstack', (0, GRANITE, 5, []))], 16)])
    # 2 chest without any BE
    put(4, 4, 2, 'chest-north')
    # 3 chest with BE but no forBlockCode and no inventory (expected NRE -> discarded)
    put(6, 4, 2, 'chest-north', 'GenericTypedContainer', [('type', 'str', 'normal-generic')])
    # 4 chest minimal: forBlockCode only
    put(8, 4, 2, 'chest-north', 'GenericTypedContainer', [('forBlockCode', 'str', 'chest-north')])
    # 5 ground sign, text + color + angle
    put(2, 4, 4, 'sign-ground-north', 'Sign', [('text', 'str', 'Hello\nOdasio'), ('color', 'int', -16777216),
                                               ('meshAngle', 'float', -math.pi / 2)])
    # 6 wall sign, text only
    put(4, 4, 4, 'sign-wall-south', 'Sign', [('text', 'str', 'Wall sign')])
    # 7 door + upper multiblock part
    put(2, 4, 6, 'door-solid-oak', 'Generic', [('rotateYRad', 'float', -math.pi / 2), ('opened', 'bool', True),
                                               ('invertHandles', 'bool', True)])
    put(2, 5, 6, 'multiblock-monolithic-0-p1-0')
    # 8 door minimal (no attributes) + part
    put(4, 4, 6, 'door-solid-oak', 'Generic')
    put(4, 5, 6, 'multiblock-monolithic-0-p1-0')
    # 9 trapdoor top half, horizontal attach north
    put(6, 4, 6, 'trapdoor-solid-oak-1', 'Generic', [('attachedFace', 'int', 0), ('rotDeg', 'int', 180),
                                                     ('opened', 'bool', False)])
    # 10 bed (head has the BE, feet has none) - VS head-south: feet at head+south
    put(2, 4, 8, 'bed-wood-head-south', 'Bed')
    put(2, 4, 9, 'bed-wood-feet-south')
    # 11 lantern
    put(4, 4, 8, 'lantern-small-up', 'Lantern', [('material', 'str', 'iron'), ('lining', 'str', 'plain'),
                                                 ('glass', 'str', 'quartz')])
    # 12 flower pot with a flower
    put(6, 4, 8, 'flowerpot-red-fired', 'PlantContainer', [
        inv([('0', 'itemstack', (0, ids['flower-lupine-blue-free'], 1, []))], 1)])
    # 13 torch minimal
    put(8, 4, 8, 'torch-basic-lit-up', 'Torch')
    # 14 sapling minimal + registered in mapchunk NewBlockEntities
    put(2, 4, 10, 'sapling-oak-free', 'Sapling'); newbe.append((X0 + 2, 4, Z0 + 10))
    # 15 unknown class name on a plain block
    put(4, 4, 10, 'rock-granite', 'Foobar')
    # 16 class mismatch: Sign BE on a granite block
    put(6, 4, 10, 'rock-granite', 'Sign', [('text', 'str', 'ghost')])
    # 17 BE without blockCode (game must resolve it from the block at pos)
    put(8, 4, 10, 'lantern-small-up', 'Lantern', blockcode=None)
    # 18 farmland with nutrients
    put(2, 4, 12, 'farmland-moist-medium', 'Farmland', [('n', 'float', 50.0), ('p', 'float', 50.0), ('k', 'float', 50.0),
                                                         ('originalFertility', 'int', 50)])
    # 19 clay oven minimal
    put(4, 4, 12, 'clayoven-north', 'Oven')
    # 20 barrel minimal
    put(6, 4, 12, 'barrel', 'Barrel')
    # 21 wild beehive without 'emptyskeps' (expected NRE)
    put(8, 4, 12, 'wildbeehive-medium', 'Beehive')
    # 22 banner clutter
    put(2, 4, 14, 'banner-banner', 'Generic', [('type', 'str', 'hansa-ground-fish'), ('meshAngle', 'float', 0.0)])
    put(2, 5, 14, 'multiblock-monolithic-0-p1-0'); put(2, 6, 14, 'multiblock-monolithic-0-p2-0')
    # 23 firepit minimal
    put(4, 4, 14, 'firepit-extinct', 'Firepit')
    # 24 storage vessel minimal
    put(6, 4, 14, 'storagevessel-red-fired', 'GenericTypedContainer', [('forBlockCode', 'str', 'storagevessel-red-fired')])
    rain = column(cx, cz, place, bes, light=(cpass == 6))
    mapchunk(cx, cz, rain, cpass, newbe)

print("column A (pass 3, re-saved by the server)")
build(16000, 16000, 3)
print("column B (pass 6, loaded as-is)")
build(16001, 16000, 6)
db.commit()
print("ok")
