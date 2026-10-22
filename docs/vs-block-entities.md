# Vintage Story 1.22.7 block entities in chunk blobs - writer spec

Companion of `docs/vs-save-format.md` (section 4, `ServerChunk` fields 7/8). Target: game/server **v1.22.7**.
Audience: the Odasio Rust writer (`crates/odasio-vs/src/column.rs`, `Column::encode`).

Confidence legend (same as the save-format doc):

- **[V]** verified: injected with our own encoder into a test world, loaded by `VintagestoryServer` 1.22.7, logs checked,
  the server re-saved the chunk and we decoded its output (section 7).
- **[C]** read in the decompiled C#, not exercised.
- **[U]** unverified / inferred (typically: client-side rendering, nothing was looked at in a real client).

Source refs: `Lib` = `VintagestoryLib.cs`, `API` = `VintagestoryAPI.cs`, `Ess` = `VSEssentials.cs`, `Surv` = `VSSurvivalMod.cs`,
all in `/tmp/claude-1001/-home-jibaymcs-Programming-Rust/d95dd67a-cfb0-4eb2-89ce-3c0f54030edc/scratchpad/decomp/`.
Line numbers refer to those files. Block JSONs: `~/.local/share/vintagestory/assets/survival/blocktypes/`.

Reference Python implementation (mirrors this spec): `tools/vs/vsbe.py` (TreeAttribute encode/decode, BE blob builder),
`tools/vs/be_test.py` (the injection experiment), `tools/vs/be_dump.py` (dumps field 8 of any `.vcdbs`).

---

## 0. TL;DR for the writer

1. For each block whose VS block type has an `entityClass` (the `be` hint in `assets/mapping.toml`), append one element to
   `ServerChunk` **field 8** (repeated LEN) of the chunk that contains the block, and set **field 7** = number of elements
   (field 7 is informative, the reader recomputes it). [V]
2. Element = `BinaryWriter.Write(string classname)` + `TreeAttribute.ToBytes` (section 1). Root tree must contain
   `posx`, `posy`, `posz` (int32, **absolute** world block coords) and should contain `blockCode` (string, short code,
   `game:` omitted). [V]
3. The game never creates a missing BE on load: no BE = the block exists but is inert (chest does not open, door does not
   open, sign not editable). No error is logged. [V for chest, C for the rest]
4. Minimal attribute sets per class: section 4. Two classes **crash** (BE discarded with an error) when attributes are
   missing: `GenericTypedContainer` (needs `forBlockCode` or `inventory`) and `Beehive` (needs tree `emptyskeps`). [V]
5. Do **not** write a `Torch` BE (it burns out on first load). Doors need a `multiblock-monolithic-0-p1-0` block in the
   upper cell. Several mapping.toml orientations are inverted (section 6). [V / C]

---

## 1. TreeAttribute binary format

`TreeAttribute.ToBytes(BinaryWriter)` / `FromBytes(BinaryReader)` (API 33979-34200). All integers/floats are
**little-endian** (.NET `BinaryWriter`). [V: our encoder output is byte-identical to the game's re-save for the door BE]

### 1.1 Primitives

| primitive | encoding |
|---|---|
| `string` | .NET `BinaryWriter.Write(string)`: length in **bytes** of the UTF-8 encoding as a 7-bit varint (unsigned LEB128: low 7 bits first, bit 7 = continuation), then the UTF-8 bytes. No terminator. `""` = `00`. |
| `int` | 4 bytes LE (i32) |
| `long` | 8 bytes LE (i64) |
| `float` | 4 bytes LE IEEE-754 (f32) |
| `double` | 8 bytes LE IEEE-754 (f64) |
| `bool` | 1 byte, `00`/`01` |

### 1.2 Tree

```
tree    := entry* 0x00                      // TerminateWrite: a single zero byte ends every tree (root and nested)
entry   := u8 typeId, string key, payload(typeId)
```

The reader loops `while ((b = ReadByte()) != 0)`; so the type id `0` is the terminator. Duplicate keys: last one wins
(dictionary assignment). Key order is irrelevant for the reader (the game writes insertion order). Nesting depth > 30 is
silently truncated (API 34072).

### 1.3 Type ids and payloads (`AttributeIdMapping`, API 34020-34036)

| id | class | payload |
|---|---|---|
| 1 | IntAttribute | i32 |
| 2 | LongAttribute | i64 |
| 3 | DoubleAttribute | f64 |
| 4 | FloatAttribute | f32 |
| 5 | StringAttribute | string (null is written as `""`) |
| 6 | TreeAttribute | nested `tree` (entries + `00`) |
| 7 | ItemstackAttribute | `bool isNull`; if `00` (not null): `i32 class` (0 = Block, 1 = Item, `EnumItemClass` API 133976), `i32 id`, `i32 stacksize`, then the stack attributes as a nested `tree` (empty = `00`). If `01`: nothing follows. (API 32564-32645, ItemStack.ToBytes API ~134745) |
| 8 | ByteArrayAttribute | **u16** length (read back as `ReadInt16`, so keep it <= 32767), then the bytes |
| 9 | BoolAttribute | u8 |
| 10 | StringArrayAttribute | i32 count, then `count` strings |
| 11 | IntArrayAttribute | i32 count, then `count` i32 |
| 12 | FloatArrayAttribute | i32 count, then `count` f32 |
| 13 | DoubleArrayAttribute | i32 count, then `count` f64 |
| 14 | TreeArrayAttribute | i32 count, then `count` nested trees (each with its own `00`) |
| 15 | LongArrayAttribute | i32 count, then `count` i64 |
| 16 | BoolArrayAttribute | i32 count, then `count` u8 |

Item/block ids inside item stacks are the **world's** runtime ids: `SaveGame.ModData["BlockIDs"]` for class 0,
`ModData["ItemIDs"]` for class 1 (see vs-save-format.md 8.1). [V: a granite stack `class=0 id=4605` in a chest survived
load + re-save]

### 1.4 Example: ground sign blob (field 8 element)

`Sign` at (512002, 4, 512004), `blockCode = "sign-ground-north"`, `text = "Hi"`, `meshAngle = -pi/2`:

```
0000  04 53 69 67 6e                                   string "Sign"            (classname, before the tree)
0005  01 04 70 6f 73 78 02 d0 07 00                    int    "posx" = 512002   (0x0007d002)
000f  01 04 70 6f 73 79 04 00 00 00                    int    "posy" = 4
0019  01 04 70 6f 73 7a 04 d0 07 00                    int    "posz" = 512004
0023  05 09 62 6c 6f 63 6b 43 6f 64 65                 string "blockCode"
002e  11 73 69 67 6e 2d 67 72 6f 75 6e 64 2d 6e 6f 72 74 68        = "sign-ground-north" (17 bytes)
0040  05 04 74 65 78 74 02 48 69                       string "text" = "Hi"
0049  04 09 6d 65 73 68 41 6e 67 6c 65 db 0f c9 bf     float  "meshAngle" = -1.5707964
0058  00                                               end of root tree
```

Chest with one stack (nested tree + itemstack), tail only:

```
... 06 09 "inventory"            tree
        01 06 "qslots" 10 00 00 00        int 16
        06 05 "slots"                     tree
              07 01 "0"                   itemstack
                 00                       isNull = false
                 00 00 00 00              class 0 (Block)
                 fd 11 00 00              id 4605
                 05 00 00 00              stacksize 5
                 00                       stack attributes: empty tree
              00                          end "slots"
        00                                end "inventory"
    00                                    end root
```

### 1.5 Rust sketch

```rust
fn w7(out: &mut Vec<u8>, mut n: usize) { while n >= 0x80 { out.push((n as u8 & 0x7f) | 0x80); n >>= 7; } out.push(n as u8); }
fn wstr(out: &mut Vec<u8>, s: &str) { w7(out, s.len()); out.extend_from_slice(s.as_bytes()); }

pub enum Attr { Int(i32), Long(i64), Double(f64), Float(f32), Str(String), Tree(Vec<(String, Attr)>),
                Stack(Option<(i32, i32, i32, Vec<(String, Attr)>)>), Bytes(Vec<u8>), Bool(bool),
                IntArr(Vec<i32>), StrArr(Vec<String>) /* ... */ }

fn wtree(out: &mut Vec<u8>, t: &[(String, Attr)]) {
    for (k, v) in t {
        out.push(match v { Attr::Int(_) => 1, Attr::Long(_) => 2, Attr::Double(_) => 3, Attr::Float(_) => 4,
                           Attr::Str(_) => 5, Attr::Tree(_) => 6, Attr::Stack(_) => 7, Attr::Bytes(_) => 8,
                           Attr::Bool(_) => 9, Attr::StrArr(_) => 10, Attr::IntArr(_) => 11 });
        wstr(out, k);
        match v {
            Attr::Int(x) => out.extend(x.to_le_bytes()), Attr::Long(x) => out.extend(x.to_le_bytes()),
            Attr::Double(x) => out.extend(x.to_le_bytes()), Attr::Float(x) => out.extend(x.to_le_bytes()),
            Attr::Str(s) => wstr(out, s), Attr::Tree(t) => wtree(out, t), Attr::Bool(b) => out.push(*b as u8),
            Attr::Bytes(b) => { out.extend((b.len() as u16).to_le_bytes()); out.extend(b); }
            Attr::Stack(None) => out.push(1),
            Attr::Stack(Some((class, id, size, attrs))) => { out.push(0); out.extend(class.to_le_bytes());
                out.extend(id.to_le_bytes()); out.extend(size.to_le_bytes()); wtree(out, attrs); }
            Attr::IntArr(a) => { out.extend((a.len() as i32).to_le_bytes()); for x in a { out.extend(x.to_le_bytes()); } }
            Attr::StrArr(a) => { out.extend((a.len() as i32).to_le_bytes()); for s in a { wstr(out, s); } }
        }
    }
    out.push(0);
}

/// One ServerChunk field-8 element.
fn be_blob(class: &str, x: i32, y: i32, z: i32, block_code: &str, extra: Vec<(String, Attr)>) -> Vec<u8> {
    let mut t = vec![("posx".into(), Attr::Int(x)), ("posy".into(), Attr::Int(y)), ("posz".into(), Attr::Int(z)),
                     ("blockCode".into(), Attr::Str(block_code.into()))];
    t.extend(extra);
    let mut out = Vec::new(); wstr(&mut out, class); wtree(&mut out, &t); out
}
// chunk protobuf: tag(7,varint)=count, then for each blob tag(8,LEN)=blob   (0x38 <count>, 0x42 <len> <blob>)
```

---

## 2. How chunk loading instantiates block entities

### 2.1 Write side (what the game itself does) [C]

`ServerChunk.FastSerializeBlockEntities` (Lib 46240): for every live BE, `BinaryWriter.Write(classname)` where classname =
`ClassRegistry.blockEntityTypeToClassnameMapping[type]` (the registered name, e.g. `"Sign"`), then
`new TreeAttribute()` filled by `be.ToTreeAttributes(tree)`, `tree.ToBytes(writer)`; written as one LEN element of field 8.
`BlockEntity.ToTreeAttributes` (API 124763) always starts with `posx`, `posy` (= `Pos.InternalY`, i.e. Y for dimension 0),
`posz`, `blockCode` (`Block.Code.ToShortString()`), then each behavior's `ToTreeAttributes`, then the subclass fields.

### 2.2 Read side (`ServerChunk.AfterDeserialization`, Lib 45826-45907) [C, outcomes V]

For each field-8 element:

1. `ReadString()` classname. Unreadable => `"Badly corrupted BlockEntity data in a chunk. Will discard it."`.
2. `TreeAttribute.FromBytes`.
3. `ClassRegistry.CreateBlockEntity(classname)` (Lib 93795): name first passed through `legacyBlockEntityClassNames`, then
   looked up in the registered names. Unknown => exception => **BE discarded** with
   `[Error] Failed loading blockentity <class> for block <code> in a chunk. Will discard it.` [V with `"Foobar"`].
4. Block resolution: `block = GetBlock(tree.blockCode)`; if `blockCode` is absent or unknown, the **block at
   (posx,posy,posz)** in this chunk is used and `blockCode` is written back into the tree. If still null:
   notification `"Block entity with classname ... has a block that is null ... Won't load"` and the BE is dropped.
   [V: a `Lantern` BE without `blockCode` loaded and was re-saved with `blockCode = "lantern-small-up"`]
5. `be.CreateBehaviors(block)` (from the block JSON `entityBehaviors`), `be.FromTreeAttributes(tree)`, stored in the
   chunk's dictionary under `be.Pos` (= posx/posy/posz). Any exception in `FromTreeAttributes` => BE discarded with the
   same error line (stack trace in `Logs/server-debug.log`). [V: chest without `forBlockCode`/`inventory`, beehive
   without `emptyskeps`]
6. When the column is handed to the game (`MTT-ChunkLoaded`, Lib 78253-78283): `be.Initialize(api)` for every BE; if the
   position is listed in the mapchunk's `NewBlockEntities` (ServerMapChunk field 9), it is removed from that set and
   `be.OnBlockPlaced(null)` is called (this is how worldgen-spawned BEs get initialised). Exceptions in `Initialize` are
   logged (`Exception thrown when trying to initialize a block entity`) and the BE stays but stops ticking.

Consequences:

- **No consistency check between the BE and the block at its position.** The class name is not compared with the
  block's `entityClass`, and `blockCode` wins over the block actually stored at the position. [V: a `Sign` BE written
  on a `rock-granite` cell loaded and was re-saved; it is an orphan forever] => only write BEs for blocks that have an
  `entityClass`, and always write `blockCode` equal to the block written at that cell.
- **A block with an `entityClass` but no BE stays without BE.** Nothing creates it lazily on load or on interaction
  (BEs are only spawned on block placement/exchange, Lib 89116-89180, and by worldgen). [V: a `chest-north` without BE
  loaded, no BE was created, no log line]. The block renders with its default shape but every BE-driven feature is
  missing (container GUI, sign text/editing, door opening, lantern colour...). [C/U]
- The BE must be stored in the chunk that contains `(posx>>5, posy>>5, posz>>5)`; lookups (`GetBlockEntity`) only search
  that chunk's dictionary. [C]
- Field 7 (`BlockEntitiesCount`) is overwritten by `BlockEntities.Count` after loading; writing it is harmless and
  matches the game. [C]
- The client never sees our bytes: the server sends its own `ToTreeAttributes` output. So client-only keys (e.g. Quern
  `clientIdsGrinding`) need not be written. [C]

### 2.3 Which BEs are mandatory

Mandatory = every block whose type has an `entityClass` **and** whose behaviour we want (interaction, rendering of
BE-driven shapes). Note that `entityBehaviors` without `entityClass` implies `entityClass = "Generic"` (Ess 21431), e.g.
`banner-*`. `scratchpad/entities.json` misses those implicit ones.

Recommended per Odasio `be` hint (counts = number of `[[block]]` rules in mapping.toml carrying the hint):

| `be` hint | rules | VS codes produced | write BE? |
|---|---|---|---|
| Sign | 120 | sign-ground-north, sign-wall-* | **yes** (text) |
| Bed | 64 | bed-wood-head-* | yes (minimal; sleeping needs it) |
| Generic | 44 | door-*, trapdoor-*, fire, supportchain-four | **yes** for doors/trapdoors; see 4.10 for the others |
| PlantContainer | 38 | flowerpot-red-fired | yes (minimal or with contents) |
| GenericTypedContainer | 30 | chest-*, storagevessel-red-fired | **yes** (else the container cannot be opened) |
| Oven | 24 | clayoven-* | yes (minimal) |
| Lantern | 12 | lantern-small-up/down | yes (material) |
| Torch | 11 | torch-basic-lit-* | **no** (see 4.8) |
| Sapling | 10 | sapling-*-free | yes, with `NewBlockEntities` (4.11) |
| Barrel | 10 | barrel | yes (minimal) |
| Farmland | 8 | farmland-* | yes (with nutrients) |
| BerryBush | 6, PumpkinVine 4, Firepit 4, Transient 3, GuiConfigurableCommands 3, Anvil 3, Quern 2, LightningRod, ItemFlow, CoalPile, Beehive | | see 4.12 |

Only `Sign` (120) and `Bed` (64) reach 50 rules.

---

## 3. Coordinates and angles - common conventions

- `posx/posy/posz` = absolute VS block coordinates (the same as the chunk position * 32 + local), int32. [V]
- VS "yaw" angles stored by BEs (`meshAngle`, `rotateYRad`) are **radians, float32**, measured as
  `atan2(dx, dz)` of the horizontal vector pointing **from the block towards the side the object faces** (the placing
  player's side): south (+Z) = 0, east (+X) = +pi/2, north (-Z) = pi, west (-X) = -pi/2. (Door `getRotateYRad` Surv 19198,
  sign `TryPlaceBlock` Surv 73751, chest `BlockGenericTypedContainer.DoPlaceBlock` (class at Surv 63552): identical formula `atan2(player.X - hit.X, player.Z - hit.Z)`.)
  Any value is accepted on load; the game itself produces multiples of pi/2 (doors), pi/4 (signs) or pi/8 (chests).
  Values in (-pi, pi] are what the game writes (`(float)Math.PI` = `0x40490fdb`). [C]
- MC -> VS for an object whose MC `facing` is the direction its **front** looks at (chests, standing sign rotation):
  `angle = atan2(fx, fz)` with (fx, fz) the MC facing unit vector => south 0, east pi/2, north pi, west -pi/2.
- BlockFacing indices (`BlockFacing.ALLFACES`, API 8943): north 0, east 1, south 2, west 3, up 4, down 5.

---

## 4. Per-class attributes

Every BE also gets the base keys `posx, posy, posz, blockCode` (section 2.1), not repeated below. "Read" lists every key
the class (and its behaviors) reads in `FromTreeAttributes`; default = value when absent.

### 4.1 Sign - `BlockEntitySign` (Surv 45720), blocks `sign-ground-*`, `sign-wall-*`

| key | type | default | meaning |
|---|---|---|---|
| `text` | string | `""` | full text, lines separated by `\n` [V round-trip; line wrapping on render U] |
| `color` | int | 0 => black (`ColorUtil.BlackArgb` = `0xFF000000` = -16777216) | text colour, ARGB as i32 |
| `meshAngle` | float | `Block.Shape.rotateY` in rad | ground sign: free rotation of post+board; wall sign: should equal the variant's rotation |
| `fontSize` | float | block attr `fontSize` (20) | |
| `translateable` | bool | false | text is a lang key |

Minimal: `text` (+ `meshAngle` for ground signs, + `color` if not black).

- **Ground signs**: write block `sign-ground-north` (as mapping.toml does) and `meshAngle = -rotation * pi/8`
  normalised into (-pi, pi] (MC `rotation` 0..15, 0 = text faces south, clockwise: 4 west, 8 north, 12 east).
  rotation 0 -> 0, 4 -> -pi/2, 8 -> pi, 12 -> +pi/2. 22.5 degree steps are finer than what the game places (45),
  accepted by the format, rendering of odd steps [U].
- **Wall signs**: omit `meshAngle`; the game derives it from the variant's shape rotateY (north 0, east 3pi/2, south pi,
  west pi/2). [V: `sign-wall-south` without `meshAngle` was re-saved with `meshAngle = pi`]
- Text: MC 1.20+ stores `front_text.messages` as 4 JSON text components; flatten each to plain text, join the 4 lines
  with `\n`, trim trailing empty lines. Back text and glowing are lost. VS board: `maxWidth 200`, `fontSize 20` [U: long
  MC lines may wrap].
- MC dye colour -> ARGB (`0xFF000000 | rgb`, MC `DyeColor.textColor`): white FFFFFF, orange FF681F, magenta FF00FF,
  light_blue 9AC0CD, yellow FFFF00, lime BFFF00, pink FF69B4, gray 808080, light_gray D3D3D3, cyan 00FFFF,
  purple A020F0, blue 0000FF, brown 8B4513, green 00FF00, red FF0000, black 000000 (default).

### 4.2 Door - class `Generic` + behavior `BEBehaviorDoor` (Surv 18938), blocks `door-*` (wood: woodtyped/door.json, metal: metal/door.json)

| key | type | default | meaning |
|---|---|---|---|
| `rotateYRad` | float | 0 | yaw of the closed door (section 4.4) |
| `opened` | bool | false | open state |
| `invertHandles` | bool | false | mirrored door = hinge on the right (section 4.4) |
| `leftDoorPosX/Y/Z`, `rightDoorPosX/Y/Z` | int x3 | absent = null | offset (relative) to the paired double-door wing, optional |
| `storyLockedCode` | string | null | story lock, never write |

Minimal for a correct door: `rotateYRad`, `invertHandles`, `opened` (all three always written by the game).
[V: a door written with only the base keys loaded fine and was re-saved with `0/false/false`; the full one round-tripped
byte-identical]

**Multiblock**: a VS door is one block at the bottom cell; the other cells of its `width x height` box are filled with
`multiblock-monolithic-<dx>-<dy>-<dz>` blocks (`BlockBehaviorDoor.placeMultiblockParts`, Surv 10837), where
`<d>` = `0`, `p<n>` or `n<n>` = offset of the part **from the door block**. For 1-wide 2-high doors (all `door-solid-*`,
`door-1x2heavy-*`) that is a single `multiblock-monolithic-0-p1-0` at y+1. These are not recreated on load (only on
placement), so the converter must write them: the MC `half=upper` cell must become `multiblock-monolithic-0-p1-0`, not
air (mapping.toml currently maps it to `game:air`). [C; loading with the part verified V, collision/selection of the
upper half U]. For wider gates the part offsets are `getAdjacentOffset(i, k, j)` (Surv 19049) with i,k < width,
j < height.

### 4.3 Trapdoor - class `Generic` + behavior `BEBehaviorTrapDoor` (Surv 21176), blocks `trapdoor-*`

| key | type | default | meaning |
|---|---|---|---|
| `attachedFace` | int | 0 | BlockFacing index of the face the trapdoor was attached to |
| `rotDeg` | int | 0 | rotation in degrees around the attachment normal |
| `opened` | bool | false | |

MC -> VS (horizontal-attachment representation, [C], visual [U]):

```
attachedFace = {north:0, east:1, south:2, west:3}[mc.facing]
rotDeg       = (mc.half == "top") ? 180 : 0
opened       = mc.open
```

Reasoning: for a horizontal `attachedFace` A, `facingWhenClosed = DOWN rotated by RotZ(rotDeg)` = down (bottom slab) for 0,
up (top slab) for 180, and `facingWhenOpened = A.Opposite`; MC `facing=F` open trapdoor stands against the side
opposite F (hinge edge), e.g. facing=north open -> z 13..16 (south). The game itself uses `attachedFace = 4 (up)` when you
click a floor; both representations are valid save data. [V: `attachedFace=0, rotDeg=180` loaded and round-tripped]

### 4.4 Door orientation, hinge and open state from Minecraft

MC door blockstate: `facing` (direction the player looked when placing; the closed panel lies on the side **opposite**
to `facing`, i.e. the placer's side: facing=north -> panel at z 13..16), `hinge` (left/right as seen by that player),
`open`, `half`.

VS: `rotateYRad` is in **radians** (float32). `rotateYRad = 0` <=> placer stood south <=> closed panel on the south
side of the cell (`collisionbox z1 0.875..1`, and `facingWhenClosed = HorizontalFromYaw(rotateYRad)` = SOUTH).

```
rotateYRad = { north: 0.0, west: +pi/2 (1.5707964), south: pi (3.1415927), east: -pi/2 (-1.5707964) }[mc.facing]
           = atan2(-fx, -fz)        with (fx, fz) = MC facing unit vector
invertHandles = (mc.hinge == "right")
opened        = (mc.open == "true")
```

Hinge: with `invertHandles = false` the door opens towards `facingWhenClosed.GetCW()` (closed south -> open panel on the
west side, i.e. the placer's left when looking north) = MC `hinge=left` (MC facing=north, hinge=left, open -> panel at
x 0..3, west). The game sets `invertHandles = true` on the right-hand wing of a double door (Surv 19097). [C]

Double doors (two MC doors side by side, same facing, hinges left+right): optional, set on the left wing
`rightDoorPosX/Y/Z` = offset to the right wing and on the right wing `leftDoorPosX/Y/Z` = offset back, so they open
together. Without them each wing opens separately. [C]

Lower/upper: write the door block + BE only for `half=lower`, the multiblock part for `half=upper` (4.2).

### 4.5 Bed - `BlockEntityBed` (Surv 26021), BE only on `bed-*-head-*` (`entityClassByType "*-head-*": "Bed"`)

Reads `mountedByEntityId` (long, 0) and `mountedByPlayerUid` (string, null). Minimal: base keys only. [V]
Feet blocks have no BE.

Orientation (section 6): VS `bed-*-{head|feet}-X` = the other half is at **+X from the head** (BlockBed.TryPlaceBlock: head
at feet + D, code side = D.Opposite). MC bed `facing` points from foot to head => **VS side = opposite(MC facing)**.

### 4.6 Chest / storage vessel - `BlockEntityGenericTypedContainer` (Surv 34881), blocks `chest-*`, `storagevessel-*`, ...

| key | type | default | meaning |
|---|---|---|---|
| `forBlockCode` | string | - | block code used to init the inventory size/type. **Required unless `inventory` is present** (else NRE -> BE discarded) [V] |
| `forBlockId` | int | - | legacy alternative to `forBlockCode` (runtime id), do not use |
| `type` | string | block attr `defaultType` (`"normal-generic"` for chests) | chest variant (`normal-generic`, `golden`, `owl`, `normal-aged`, ...) selects shape/slots |
| `meshAngle` | float | 0 | yaw of the rendered model; **the block's side variant does not orient the model**, the BE mesh is `Rotate(0, meshAngle, 0)` |
| `isPerPlayer` | bool | false | loot per player, never write |
| `inventory` | tree | absent = empty | `{ qslots: int, slots: tree { "<i>": itemstack } }` |

Minimal for an openable, correctly oriented empty chest: `forBlockCode` (= the block code), `type = "normal-generic"`,
`meshAngle`. [V: a BE with only `forBlockCode` loaded and was re-saved with an empty 16-slot inventory]

- `meshAngle` from MC chest `facing` (front direction): south 0, east pi/2, north pi, west -pi/2 (section 3). Without it
  every chest faces south. [C, visual U]
- Inventory: `qslots` is ignored when `forBlockCode` is present (slots come from the block: 16 for `normal-generic`); only
  keys `"0".."15"` are read, extra MC slots (27, or 54 for double chests) are lost. Slot value = itemstack
  `(class, id, size, attrs)`, ids from the world's BlockIDs / ItemIDs. Empty slots are simply absent. [V]
- MC double chests (`type=left/right`): VS has no double chest; two independent chests.
- Storage vessel: same class, `forBlockCode = "storagevessel-red-fired"`. [V]

### 4.7 Lantern - `BELantern` (Surv 40280), blocks `lantern-*`

| key | type | default |
|---|---|---|
| `material` | string | `"copper"` (valid: copper, brass, blackbronze, bismuth, tinbronze, bismuthbronze, iron, molybdochalkos, silver, gold, steel, meteoriciron, electrum) |
| `lining` | string | `"plain"` |
| `glass` | string | `"quartz"` (glass colour also tints the light) |
| `meshAngle` | float | 0 |

Minimal: base keys (gives a copper lantern). Suggested: `material = "iron"` for MC lantern. [V]

### 4.8 Torch - `BlockEntityTorch` : `BlockEntityTransient` (Surv 51192, Ess 27041), blocks `torch-*-lit-*`

Reads `transitionHoursLeft` (double), `transitionAtTotalDays` (double, legacy), `lastCheckAtTotalDays` (double),
`convertToOverride` (string). With a minimal BE, `lastCheckAtTotalDays = 0` makes the first `CheckTransition` replay every
hour since day 0 and the torch burns out immediately. [V: `torch-basic-lit-up` came back as `torch-basic-burnedout-up`,
BE gone]

**Recommendation: do not write a Torch BE.** Without BE the lit torch never transitions (no burn-out, no
temperature/rain extinguishing), which matches MC permanent torches. [C] (If burn-out is wanted: write
`lastCheckAtTotalDays` = world `Calendar.TotalDays` and `transitionHoursLeft` = 48.)

### 4.9 Flower pot - `BlockEntityPlantContainer` (Surv 42688), block `flowerpot-*`

Reads the container `inventory` tree (1 slot) and `meshAngle` (float). Minimal: base keys (empty pot). [V]
Contents: `inventory = { qslots: 1, slots: { "0": itemstack(class 0, id of a plantable block, 1, {}) } }`, e.g.
`flower-lupine-blue-free`. [V load/round-trip; rendering U]. MC `potted_*` -> VS flower code mapping is a separate table
(mapping.toml currently maps potted plants to an empty pot).

### 4.10 Clay oven, bookshelf, banner, other `Generic` blocks

- **Oven** (`BlockEntityOven`, Surv 129649; `clayoven-*`): reads container `inventory`, `qslots`/`slots` (oven inv), `burn`,
  `rota` (ignored: rotation comes from the `side` variant in `Initialize`), `temp`, `tfuel`, `brown<i>/tbake<i>/baked<i>/
  heightmul<i>/temp<i>`. Minimal: base keys. [V]
- **Bookshelf**: `bookshelves-ns/-we` (legacy) has **no** entityClass => no BE. [C] (`bookshelf-clutter*` would be `Generic`.)
- **Banner**: `banner-banner` is a Hansa decorative clutter: class `BlockClutter`, implicit `Generic` + behavior
  `ShapeFromAttributes` (Surv 20498). Keys: `type` (string, required to render anything: one of `hansa-ground-fish`,
  `hansa-ground-keys`, `hansa-ground-largefish`, `hansa-ground-ship`, `hansa-ground-town`, `hansa-wall-*` same suffixes),
  `meshAngle` (float), `rotateX/rotateZ`, `offsetX/Y/Z`, `overrideTextureCode`, `collected`, `repairState`. It is also a
  1x3x1 multiblock (`Multiblock` behavior, controller at the bottom) => write `multiblock-monolithic-0-p1-0` and
  `-0-p2-0` above. MC colours/patterns are not representable (no patterned banners in vanilla 1.22.7). [V load of
  `type=hansa-ground-fish`; rendering U]. Consider mapping MC banners to a cloth/clutter block instead.
- **fire** (`Generic` + `Burning`): minimal BE = not burning, `fromFacing` 0. Map MC `fire` to air instead. [C]
- **supportchain-four** (`Generic` + `SupportBeam`): beams are stored in `beams` (bytes, protobuf `PlacedBeam[]`);
  without it the block has no beam geometry [U]. Avoid as a target, or skip the BE.

### 4.11 Sapling - `BlockEntitySapling` (Surv 44648)

Reads `totalHoursTillGrowth` (double, 0), `growthStage` (int, default 1 = Sapling), `plantedFromSeed` (bool). With 0 the
tree grows at the first check (2 s tick, temperature >= 5 C). Best option: write the minimal BE **and add its position to
the mapchunk `NewBlockEntities` (ServerMapChunk field 9, repeated LEN `BlockPos {1:X, 2:Y, 3:Z}`)**: on first load the
game calls `OnBlockPlaced()` which sets `growthStage = 1` and `totalHoursTillGrowth = now + growth time`.
[V: re-saved with `totalHoursTillGrowth = 1206.6` (world TotalHours ~872), field 9 consumed]. If growth is unwanted,
omit the BE (sapling stays forever).

### 4.12 Other classes in mapping.toml (minimal = base keys unless stated)

| class | block | reads / notes | minimal OK? |
|---|---|---|---|
| Barrel | barrel | container `inventory` (2 slots), `sealed`, `sealedSinceTotalHours` | yes [V] |
| Farmland | farmland-* | soil: `n,p,k` (float), `slowN/P/K`, `moistureLevel`, `originalFertility` (int, or `originalFertilityN/P/K`), `totalHoursForNextStage`, `cropAttrs` (tree), ... Without `n,p,k` the soil is at 0 nutrients. Write `n=p=k=originalFertility=50` for `*-medium` (Fertilities: verylow 5 ... medium 50, Surv 46374). Moisture is recomputed (moist -> dry when no water nearby) | yes, with nutrients [V] |
| Firepit | firepit-* | container inv (7 slots), temperatures, `fuelBurnTime`, `extinguishedTotalHours`, `canIgniteFuel`... `firepit-extinct` became `firepit-cold` (extinguished long ago); `firepit-lit` without fuel will go out | yes [V]; map MC campfire to `firepit-cold`/`-extinct` |
| BerryBush | smallberrybush-* | `transitionHoursLeft`, `lastCheckAtTotalDays`, `roomness`, `pruned`, `lastPrunedTotalDays`; 0 => recomputed in Initialize (no catch-up) | yes [C] |
| PumpkinVine | pumpkin-vine-* | `totalHoursForNextStage` (0 => grows at once), `canBloom`, `north/east/south/west`, `parentPlantPosX/Y/Z`, `preferredGrowthDir`, `internalStage` | yes, grows immediately [C] |
| Transient | tallgrass-eaten-free | like Torch: catches up from day 0 and converts at once to its target | skip BE / map to plain tallgrass [C] |
| GuiConfigurableCommands | commandblock-normal | `commands` (string), `silent`, `callingPrivileges` (string[]) | yes; better not to convert command blocks [C] |
| Anvil | anvil-* | `voxels` (bytes, null ok), `workItemStack`, `selectedRecipeId`, `rotation`, `meshAngle` | yes [C] |
| Quern | quern-* | container inv, `inputGrindTime`, `nowOutputFace` | yes [C] |
| LightningRod | lightningrod | nothing | yes [C] |
| ItemFlow | hopper | `lastReceivedFromDir` (int, write -1 for none), container inv | yes [C] |
| CoalPile | coalpile | `inventory` (item pile: coal stacks decide the height), `burning`, `lastTickTotalHours`, `isCokable` | needs an inventory with coal items to be meaningful [U] |
| Beehive | wildbeehive-* | **requires tree `emptyskeps`** (may be empty) else NRE; `scanIteration`, `quantityNearbyFlowers`, `isWildHive`, `harvestable`... | write `emptyskeps = {}` [V for the crash] |

---

## 5. Decision summary for Odasio

| MC block | VS block(s) | BE to write |
|---|---|---|
| `*_sign` (standing) | `sign-ground-north` | `Sign {text, color?, meshAngle = -rot*pi/8}` |
| `*_wall_sign` facing F | `sign-wall-<opposite(F)>` | `Sign {text, color?}` |
| `*_door` lower, facing F, hinge H, open O | `door-...` | `Generic {rotateYRad(F), opened=O, invertHandles=(H==right)}` |
| `*_door` upper | `multiblock-monolithic-0-p1-0` | none |
| `*_trapdoor` facing F, half, open | `trapdoor-...-1` | `Generic {attachedFace=idx(F), rotDeg=0|180, opened}` |
| `*_bed` head facing F | `bed-wood-head-<opposite(F)>` | `Bed {}` |
| `*_bed` foot facing F | `bed-wood-feet-<opposite(F)>` | none |
| `chest`/`trapped_chest`/`barrel`... facing F | `chest-<F>` | `GenericTypedContainer {forBlockCode, type:"normal-generic", meshAngle(F), inventory?}` |
| `lantern` | `lantern-small-up/down` | `Lantern {material:"iron"}` |
| `torch`, `wall_torch` | `torch-basic-lit-*` | **none** |
| `flower_pot`, `potted_*` | `flowerpot-red-fired` | `PlantContainer {inventory?}` |
| `furnace` | `clayoven-*` | `Oven {}` |
| saplings | `sapling-*-free` | `Sapling {}` + mapchunk field 9 entry (or none) |
| `farmland` | `farmland-*-medium` | `Farmland {n,p,k:50, originalFertility:50}` |
| `beehive`/`bee_nest` (if mapped to wildbeehive) | | `Beehive {emptyskeps:{}}` |

---

## 6. Problems found in `assets/mapping.toml`

1. **Door upper half -> `game:air`** (all `*_door` with `half = "upper"`): must be `game:multiblock-monolithic-0-p1-0`
   (section 4.2). [C]
2. **Wall signs inverted**: `oak_wall_sign facing=north -> sign-wall-north` but VS `sign-wall-north` has its board on the
   north edge of the cell (collision box z 0..0.0625) and its text facing south. Correct: `sign-wall-<opposite(facing)>`.
   [C: BlockSign.TryPlaceBlock uses `CodeWithParts("wall", bs.Face.Opposite)`; selection boxes in sign.json]
3. **Beds inverted**: `facing=north -> bed-wood-head-north` but VS side = direction from head to feet. Correct:
   `bed-wood-{head,feet}-<opposite(facing)>`. [C: BlockBed.TryPlaceBlock, class at Surv 54380]
4. Chests: the side variant does not orient the model; the BE `meshAngle` does (4.6). [C]
5. `be = "Torch"` and `be = "Transient"` hints: do not emit the BE (4.8, 4.12). [V for Torch]
6. `banner-banner` targets need a `type`, two multiblock parts above, and lose all colours (4.10).

---

## 7. Verification run [V]

Material (scratchpad `S=/tmp/claude-1001/-home-jibaymcs-Programming-Rust/d95dd67a-cfb0-4eb2-89ce-3c0f54030edc/scratchpad`):

- `$S/be_before.vcdbs` = `$S/flat_copy.vcdbs` + `tools/vs/be_test.py $S/be_before.vcdbs $S/blockids_flat.json`.
- Server run: `WORK=$S/bework tools/vs/validate.sh $S/be_before.vcdbs 70` (dataPath under the scratchpad, never
  `~/.config/VintagestoryData`). Result world: `$S/bework/Saves/world.vcdbs`, logs `$S/bework/Logs/`.
- Dumps: `tools/vs/be_dump.py <world> 16000 16000` -> `$S/be_after_A.txt` / `$S/be_after_B.txt`.

Setup: column A (16000,16000) written with `currentpass = 3` so the server lights and **re-saves** it (the re-save is
the game's own serialisation of our BEs); column B (16001,16000) identical with `currentpass = 6` (loaded as-is, not
re-saved). 23 BEs per column on y=4 over a granite floor.

Results (identical for both columns where observable):

| case | outcome |
|---|---|
| chest with forBlockCode+type+meshAngle+inventory(5 granite) | loaded, re-saved with same values and the stack |
| chest with only forBlockCode | loaded, re-saved with 16 empty slots, `type normal-generic`, `meshAngle 0` |
| chest with only `type` (no forBlockCode, no inventory) | `[Error] Failed loading blockentity GenericTypedContainer ... discard` (NRE at BEGenericTypedContainer.cs:135) |
| chest-north block without BE | loaded, no BE created, no log |
| ground sign text/color/meshAngle | round-trip, `fontSize 20`, `translateable false` added |
| wall sign `sign-wall-south`, text only | round-trip, `meshAngle = pi` filled from the variant |
| door with rotateYRad/opened/invertHandles + multiblock part | **byte-identical** after re-save |
| door with base keys only | re-saved with `rotateYRad 0, opened false, invertHandles false` |
| trapdoor attachedFace 0 / rotDeg 180 | byte-identical |
| bed head BE (head-south + feet-south) | round-trip (+ `mountedByEntityId 0`, `mountedByPlayerUid ""`) |
| lantern material iron | round-trip (+ `meshAngle 0`) |
| lantern without blockCode | resolved from the block at pos, `blockCode` written back, copper defaults |
| flower pot with lupine stack | round-trip |
| torch minimal BE | block converted to `torch-basic-burnedout-up`, BE removed |
| sapling minimal + mapchunk field 9 | `OnBlockPlaced` ran: `totalHoursTillGrowth 1206.6`, `growthStage 1`; field 9 emptied |
| class `Foobar` | `Failed loading blockentity Foobar ... discard` |
| `Sign` BE on a granite cell | kept and re-saved (orphan) |
| farmland n/p/k 50 | kept, block became `farmland-dry-medium` (no water) |
| oven, barrel, firepit, storage vessel minimal | loaded; firepit block became `firepit-cold` |
| banner-banner type hansa-ground-fish + 2 parts | loaded and re-saved with ShapeFromAttributes keys |
| beehive minimal | `Failed loading blockentity Beehive ... discard` (NRE, missing `emptyskeps`) |

No `Exception thrown when trying to initialize a block entity` in the logs. Nothing was checked in a game client:
everything about appearance (orientation of doors/signs/chests/trapdoors, sign text layout, pot contents, banner) is [U].

---

## 8. Open risks

- Visual orientation conventions (sections 3, 4.4, 4.3, 6) are derived from code, not looked at in a client [C/U]. A
  quick client check of one door per facing x hinge, one ground sign per 45 degrees, one wall sign and one chest per
  facing would close it.
- Orphan BEs (BE whose block has no entityClass) are never cleaned up by the game: a writer bug produces permanent junk.
- Item/block ids inside item stacks depend on the template world's BlockIDs/ItemIDs (same rule as chunk block ids).
- MC data that has no VS equivalent: sign back text and glow, double chests (54 slots), chest contents beyond 16 slots,
  banner colours/patterns, chiseled bookshelf contents, potted plant species (needs a mapping table).
- Mapchunk field 9 (`NewBlockEntities`) triggers `OnBlockPlaced(null)` for any listed BE; only verified for Sapling. Do
  not list other classes without checking their `OnBlockPlaced`.
- Transient-like BEs (Torch, Transient, firepit, farmland moisture) use world time; with the template world's calendar
  (TotalDays ~36 in the test world) "time since 0" catch-ups happen on first load.
