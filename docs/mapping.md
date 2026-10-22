# Block mapping (Minecraft Java -> Vintage Story)

Two data files live in `assets/`:

| File | Content |
|------|---------|
| `assets/vs_block_codes.txt` | Every concrete Vintage Story 1.22.7 block code (14 136 codes, one per line, sorted), expanded from the game's blocktype JSON files. |
| `assets/mapping.toml` | Rules mapping Minecraft Java 1.18 - 1.21.x block ids (+ blockstate properties) to VS block codes. |

Both were produced by throwaway Python scripts (`vsexpand.py`, `genmap.py`, `validate.py`), see "Regenerating" below. After generation, `mapping.toml` is meant to be edited by hand.

## mapping.toml schema

```toml
[settings]
rock = "granite"            # substituted for "{rock}" in any `vs` code
safe_rocks = ["andesite", ...]  # rocks for which every "{rock}" code exists in VS

[default]                   # used when no [[block]] rule matches (modded / unknown ids)
solid = "game:rock-{rock}"
non_solid = "game:air"

[[block]]
mc     = "minecraft:oak_log"        # namespaced MC id (required)
when   = { axis = "y" }             # optional blockstate filter
vs     = "game:log-placed-oak-ud"   # VS code incl. domain (required)
approx = true                       # optional, default false
fluid  = "game:water-still-7"       # optional, liquid for the VS fluid layer
be     = "GenericTypedContainer"    # optional, informative: VS block-entity class
note   = "..."                      # optional
```

Matching rules:

- Rules are grouped by `mc` and evaluated **in file order; the first match wins**. Specific rules (with `when`) come before catch-all rules for the same id.
- `when`: each listed property must equal the MC palette value (always a string: `"true"`, `"3"`, `"north"`). An array means "any of", e.g. `{ level = ["8", "9", ...] }`. Properties that are not listed are ignored (e.g. `waterlogged`, `snowy`, `distance`).
- `{rock}` in `vs` is replaced by `settings.rock`. The rock has to be in `safe_rocks` (the generator checked that every templated code exists for those rocks). Rocks such as `tuff`, `obsidian` or `whitemarble` are valid VS rocks, but some families (stone bricks, gravel...) do not exist for them.
- `fluid`: also put this liquid in the VS fluid layer (kelp, seagrass, corals). Separately from this field, the converter should put `game:water-still-7` in the fluid layer for any MC block with `waterlogged = "true"`.
- `be`: the VS block has this `entityClass` (`Generic`, `GenericTypedContainer`, `Sign`, `Bed`, `Farmland`, `PlantContainer`, ...). The converter has to create a matching block entity, otherwise the block may be inert or broken in game. Doors and trapdoors (`Generic` + `BEBehaviorDoor`) store their **orientation and open state in the block entity**, not in the code.
- `[default]`: deciding whether an unknown MC block is "solid" is up to the converter (e.g. a small list or a name heuristic).

## How VS block codes are expanded

Blocktypes live in `~/.local/share/vintagestory/assets/{game,survival,creative}/blocktypes/**.json` (JSON5: unquoted keys, comments, trailing commas). There is **no domain folder level**: `game`, `survival` and `creative` are three asset *origins* of the same `game` domain, so every base-game code is `game:<code>` (lang keys are `block-rock-granite`, without a domain).

For each blocktype (`enabled: false` ones are skipped):

1. Each entry of `variantgroups` gives a list of states: `states` + the `variants[].code` of `loadFromProperties` (a file under `survival/worldproperties/`, e.g. `block/wood`, `block/rockwithdeposit`, `abstract/horizontalorientation`) + `loadFromPropertiesCombine` (several property files concatenated). A group can have both `states` and `loadFromProperties` (e.g. `wood: ["aged","veryaged"] + block/wood`).
2. Groups with `combine: "multiply"` (the default) form a cartesian product, in declaration order: `code-<g1>-<g2>-...`.
3. Groups with `combine: "add"` add standalone variants `code-<state>` (used by `door` and the metal door).
4. `allowedVariants` (keep only matching codes) and `skipVariants` (drop matching codes) use VS wildcards: `*` is a glob, and a leading `@` means a regex. They are matched case-insensitively on the code without domain.
5. `game:air` and `game:mantle` (bedrock) are hard-coded in the engine, have no JSON and were added by hand.

Known limitations of the expansion: `selectiveMultiply` (unused in 1.22.7) is treated as `multiply`, and patches (`survival/patches`) and mods are ignored. Since all mapped codes belong to plain, widely used blocktypes, this does not affect `mapping.toml`.

## VS naming conventions worth knowing

- **Orientation suffixes**: logs, planks, hay, quartz pillar, stacked bamboo use `-ud` / `-ns` / `-we` (MC `axis` y / z / x). Planks also carry an orientation (`planks-oak-ud` is the default).
- **Logs**: `log-grown-<wood>-*` is the worldgen tree log (part of a tree), `log-placed-<wood>-*` the player-placed one. `debarkedlog-*` is the stripped log. VS has no 6-sided bark block.
- **Leaves**: `leaves-grown-<wood>` decays (like MC non-persistent leaves) and `leaves-placed-<wood>` does not. `grown1..7` are visual variants.
- **Soil**: `soil-<fertility>-<grass>` with fertility `verylow|low|medium|compost|high` and grass coverage `none|verysparse|sparse|normal`. So `grass_block` maps to `soil-medium-normal` and `dirt` to `soil-medium-none`. `forestfloor-0..7` is leaf-litter covered soil.
- **Rock-typed families**: `rock-`, `cobblestone-`, `stonebricks-`, `rockpolished-`, `gravel-`, `sand-`, `crackedrock-`, `mossycobblestone-`, `drystone(fence)-`... each have their own subset of rock types. `ore-*` codes end with the host rock, and each ore only exists in some rocks (coal: sedimentary only; diamond: kimberlite only).
- **Slabs**: `<fam>-<mat>-<rot>-<cover>` with rot `down` = bottom slab, `up` = top slab, `north|east|south|west` = vertical slabs, cover `free|snow` (`up-snow` does not exist).
- **Stairs**: `<fam>-<mat>-<up|down>-<facing>-<cover>`. `up` = normal, `down` = upside down. The VS facing equals the MC `facing` (the high half is on the facing side). There are no corner shapes.
- **Fences/walls**: connections as letters in `n e s w` order, or `empty` (`woodenfence-oak-nes-free`). Gates: `woodenfencegate-<wood>-<n|w>-<closed|opened>-<left|right>-<cover>`.
- **Liquids**: `water-still-7` is a full source. `water-<dir>-<1..7>` is flowing water (`d` = falling, `n`, `ne`... = flow direction). Same for `lava-`, `saltwater-`, `boilingwater-`.
- **Wall attachments**: a wall torch `torch-basic-lit-north` leans north, like MC `wall_torch[facing=north]`. A ladder `ladder-*-north` hangs on the **north** wall, so MC `ladder[facing=north]` (attached to the south block) maps to `ladder-*-south`.
- Doors, trapdoors, chests, signs, beds, lanterns, flower pots: part or all of their state (orientation, material, contents) lives in the block entity.

## Coverage (generated 2026-10-22)

The MC block list comes from the 1.21.1 client jar blockstates (1 062 ids), plus 46 ids added by hand: 1.21.4/1.21.5/1.21.6 blocks (pale oak family, resin, eyeblossoms, bushes, leaf litter, dried ghast...) and the pre-1.20.3 legacy id `minecraft:grass`.

| Metric | Value |
|---|---|
| MC ids considered | 1 108 |
| MC ids with at least one rule | 1 108 (100 %) |
| ids with only exact rules | 168 |
| ids with some approximate rules | 14 |
| ids with only approximate rules (`approx = true`) | 926 |
| ids that map entirely to `game:air` (removed) | 147 |
| `[[block]]` rules | 2 780 (2 240 marked `approx`) |
| rules with a `be` hint | 400 |
| distinct VS codes used | 1 211 |

`approx` is set generously. It marks every substitution where a player would notice a difference: a species swap (cherry -> maple), a colour loss, a missing shape (walls -> dry-stone fences, copper stairs -> aged plank stairs), redstone removed, and so on.

Main choices:

- **Woods**: oak -> oak, spruce -> pine, birch -> birch, jungle -> kapok, acacia -> acacia, dark_oak -> walnut (exact). The following are approximate: mangrove -> baldcypress, cherry -> maple (crimsonkingmaple leaves), pale_oak -> aged, crimson -> purpleheart, warped -> ebony, bamboo planks -> larch (`bamboo_block` -> `stackedbamboo`).
- **Stone**: `stone`, `cobblestone`, `stone_bricks`, `smooth_stone`, `gravel`, walls -> `{rock}` (default granite). Named stones are fixed: granite/andesite exact; diorite -> whitemarble, calcite -> chalk, deepslate -> slate, blackstone -> basalt, dripstone -> limestone, netherrack/magma -> scoria, end stone -> chalk, prismarine -> greenmarble, purpur -> redmarble. Sand -> `sand-sandstone` (the yellow VS sand), red sand -> `sand-bauxite`.
- **Ores**: iron -> hematite (deepslate variant -> magnetite in slate), copper -> native copper, gold -> quartz + native gold, redstone -> cinnabar, lapis -> lapis lazuli in limestone, diamond -> diamond in kimberlite, emerald -> emerald in slate, coal -> bituminous coal in shale.
- **Colours** (16 dyes): wool -> VS wool (white/black/brown only, otherwise coloured daub), concrete -> daub, terracotta -> 2x2 clay tiles, stained glass -> the 10 VS glass colours, carpets -> small carpets, beds -> wooden bed (colour lost), banners -> removed.

## Hard cases / limitations

- **Host rock of ores**: VS ores are rock-typed and each ore only exists in some rocks. Fixed host rocks are used, so ore blocks can look out of place in the surrounding `{rock}`. A smarter converter could rewrite the trailing `-<rock>` to match the neighbouring stone whenever that code exists in `vs_block_codes.txt`.
- **Biomes / rock strata**: VS worlds have rock layers and climate-dependent soil. The mapping uses one global rock and `soil-medium-*`. Per-region rock choice (e.g. from MC biome or depth) would be a converter feature.
- **Block entities**: doors, trapdoors, chests, signs, beds, lanterns, flower pots, farmland, crops and saplings need VS block entities (see `be`). Door and trapdoor orientation exist **only** in the BE, so without BE support they get a default orientation. MC double chests are not merged.
- **Upper halves**: for two-block MC blocks (doors, tall grass, tall flowers, large fern), the upper half maps to air and the lower half to a single VS block. VS doors are multiblocks created from the lower block.
- **Flowing liquids**: VS needs a flow direction that cannot be derived from one MC block. Flowing water is placed as still water of decreasing height, and falling water as `water-d-7`.
- **Stairs corners** (`shape`), wall heights (`low`/`tall`/`up`), multi-face vines and glow lichen, sign rotation (16 steps -> one fixed direction), and bed/gate/chest/oven facing conventions are **not verified in game** (see the notes on each rule). Stairs, torches and ladders were checked against the VS shape files.
- **Redstone, rails, command/structure blocks, heads, banners, item frames, portals**: no VS equivalent, mostly mapped to air (`approx = true`).
- **MC versions**: the list covers 1.21.1 plus the main 1.21.4 - 1.21.6 additions. Ids from 1.21.9+ (e.g. `iron_chain`, copper torches/lanterns, shelves) and pre-1.18 legacy ids other than `grass` are not covered and fall back to `[default]`.

## Regenerating

The scripts were written as throwaway tools during the initial generation (in the agent scratchpad). Copy them into the repo (e.g. `tools/`) if regeneration matters:

```sh
python3 -I vsexpand.py ~/.local/share/vintagestory assets/vs_block_codes.txt vs_entity.json
unzip ~/.minecraft/versions/1.21.1/1.21.1.jar 'assets/minecraft/blockstates/*' -d mcdata
python3 -I mcprops.py mcdata/assets/minecraft/blockstates mc_blocks.json
python3 -I genmap.py assets/vs_block_codes.txt vs_entity.json mc_blocks.json assets/mapping.toml stats.json
python3 validate.py assets/vs_block_codes.txt assets/mapping.toml   # needs `tomli`
```

Because `genmap.py` overwrites hand edits, after the first manual edit treat `mapping.toml` as the source of truth and run only `validate.py`.

## Qualité, catégories et politiques (remplace `approx`)

Le booléen `approx` a été remplacé par deux champs (script `tools/mapping/classify.py`) :

- `quality` : `exact` (même bloc), `equivalent` (même rôle, variante VS : essence, couleur, espèce),
  `lossy` (bon bloc mais donnée perdue : texte, contenu, orientation en block entity),
  `substitute` (autre bloc en remplacement), `removed` (aucun équivalent, air).
  Un bloc sans règle est `unmapped` (calculé, jamais écrit dans le fichier).
- `category` : `terrain`, `ore`, `wood`, `plant`, `liquid`, `building`, `colored`, `light`,
  `sign`, `door`, `container`, `redstone`, `decoration`.

Résolution (`odasio-core`, `Mapping::resolve`) :

1. surcharge utilisateur par état exact (`minecraft:oak_log[axis=y]`) ou par ID ;
2. première règle dont l'ID et le filtre `when` correspondent. Une propriété absente de l'état
   (chunk d'une ancienne version) ne bloque pas la règle ;
3. repli `[default]` ;
4. action de la politique pour (qualité, catégorie) puis (qualité) : conserver, air, ou bloc VS ;
5. minerais replacés dans la roche du terrain quand la combinaison existe ;
6. liquides déplacés dans la couche fluide, `waterlogged=true` ajoute de l'eau.

`odasio plan <monde>` affiche ce que deviendra chaque bloc réellement présent, groupé par
qualité et catégorie : c'est le modèle de l'assistant de mapping de l'interface.

## Reverse mapping (Vintage Story -> Minecraft Java 1.21.1)

File: `assets/mapping_vs_to_mc.toml`, generated by `tools/mapping/gen_reverse.py` (do not edit by hand: change the tables in the script and re-run it). `python3 tools/mapping/gen_reverse.py --check` only validates the existing file.

Schema (documented at the top of the file):

```toml
[default]                       # no rule matched
solid = "minecraft:stone"       # VS light absorption 99 (opaque, VS default)
non_solid = "minecraft:air"     # anything else

[[block]]
vs = "game:ore-*-nativecopper-*"   # exact VS code, or a glob: `*` = any sequence (incl. "-")
mc = "minecraft:copper_ore"
props = { axis = "y" }             # optional, valid 1.21.1 blockstate properties
quality = "equivalent"             # exact | equivalent | substitute | removed
note = "..."                       # optional
```

Rules are evaluated in file order, first match wins. The file lists exact codes first, then per-family globs (specific before broad).

How it is built: the script holds per-family tables (rocks, woods, clay colours, metals, ores, plants...) and expands them into rules, spelling out every orientation segment (stairs `up|down` x facing, slabs, fence/wall connections, logs `ud|ns|we`, doors, beds, signs, ladders, torches...). It also inverts the `exact`/`equivalent` rules of `mapping.toml`; an inverted entry is written only when it differs from the family rule and the family is not "authoritative". In the current run, 618 of the 847 inverted candidates are identical to the family rules and 229 were deliberately overridden (see below), so no extra inverted entry is written. The in-game conventions of the forward mapping are kept: glass pane `-ns` = MC east/west, `sign-wall-X` and beds = opposite of MC facing, ladders = opposite side, stairs facing equal, slab `down` = bottom.

Validation: every MC id and property value is checked against the blockstate files of the 1.21.1 client jar, plus a small table of properties that blockstate files do not list (`waterlogged`, leaves `distance`/`persistent`, liquid `level`, sign `rotation`, bed `part`/`facing`, chest `facing`, wall side `none`, kelp/cave vines `age`).

### Coverage (generated 2026-10-22)

| Metric | Value |
|---|---|
| VS codes | 14 136 |
| rules | 5 141 (1 900 exact codes, 3 241 globs) |
| codes matched by an exact-code rule | 1 900 (13.4 %) |
| codes matched by a glob | 12 236 (86.6 %) |
| codes falling to `[default]` | 0 |
| by quality | exact 936, equivalent 5 306, substitute 5 419, removed 2 475 |
| invalid MC states / dead rules | 0 / 0 |

### Main choices and known approximations

- **Rocks** were chosen by average texture colour, not by name: VS granite is a plain grey (-> `stone`, cobble -> `cobblestone`, bricks -> `stone_bricks`), andesite/phyllite -> andesite, basalt -> `smooth_basalt` (blackstone family for cobble/bricks), shale/slate -> deepslate family, kimberlite/peridotite/suevite/scoria/tuff -> tuff family, chalk/whitemarble -> calcite (diorite/quartz for building), limestone/greenmarble -> diorite, chert -> granite, claystone -> packed mud, conglomerate -> dripstone block, bauxite -> red sandstone, halite/redmarble/travertine -> white terracotta, sandstone -> sandstone family. Ores keep the MC ore of the mineral (native copper/malachite -> copper, hematite/magnetite/limonite -> iron, gold, coal family, cinnabar -> redstone, lapis, diamond, emerald); metals without MC ore (tin, zinc, lead, silver, nickel...) -> iron ore; non-metal deposits (sulfur, borax, quartz, alum...) -> plain host rock. Shale/slate hosts give `deepslate_*_ore`. The ore grade is lost.
- **Woods**: oak, birch, acacia exact; pine -> spruce, kapok -> jungle, walnut -> dark oak (exact in the forward mapping); ebony -> dark oak, baldcypress/redwood -> mangrove, larch/aged -> spruce, maple -> cherry wood with oak leaves, crimson king maple -> cherry leaves, purpleheart -> crimson (stems; jungle leaves). Leaves get `distance=1` so MC does not decay trees before the first block update.
- **Shapes MC lacks**: vertical slabs -> full block; glass slabs -> glass block; terracotta tile slabs -> terracotta; thatch/sod roofs -> hay/moss blocks; slanted roof pieces -> stairs (corners `inner_left`/`outer_left`), ridges and tips -> bottom slabs (roof orientation unverified); layered sand/gravel: 4+ layers -> full block, less -> air; tall VS grass and tall fern (single blocks) -> `short_grass`/`fern`.
- **Colours**: clay bricks/shingles by fired colour (red/orange -> bricks, brown/tan -> mud bricks, cream/fire -> end stone bricks, gray -> stone bricks, black -> polished blackstone bricks); tiles and hardened clay -> terracotta colours; daub -> muted concrete/terracotta; creative colour blocks -> nearest concrete (from the VS textures).
- **Removed (air, 2 475 codes)**: loose stones/ores/flints/boulders, cave art and scribbles, paintings, plaques, tool moulds and small ceramics, mechanical power parts and machines, roof/support beams (geometry in the block entity), metal sheets on walls, shelf fungi on tree sides, multiblock fillers.

### What the converter (Rust side) must handle specially

- **Fluid layer**: VS liquids live in a separate layer. Merge it: water in air -> `water`, water in a waterloggable MC block -> `waterlogged=true`, otherwise keep the solid block. `still-7` = source (`level=0`), `still-N` -> `level=7-N`, falling `d-N` -> `level=8`, flowing -> `level=max(1,7-N)`.
- **Multiblocks**: `multiblock-monolithic-*` maps to air. The converter should rebuild the upper part from the controller block: door at `dy=-1` -> same door with `half=upper` (same facing/hinge/open), tall plants likewise.
- **Block entities carrying orientation/state**: doors (`door-solid-*`, `metaldoor-*`: `BEBehaviorDoor` rotateYRad, invertHandles, opened -> MC `facing`, `hinge`, `open`; lower half only in the mapping), trapdoors (`trapdoor-solid/window/bars/plate-*`), chests (`meshAngle` for `chest`/`labeledchest`/`trunk` when the code has no side), ground signs (angle -> `rotation`, text), wall signs (text), beds, flower pots (plant), barrels/vessels (contents lost), chiseled blocks and microblocks (mapped to `stone`: their material list is in the BE, the main material could be used instead), ingot/plate piles and ground storage (mapped to air).
- **Unverified orientations** (not tested in game): ground sign rotation, fence gate axis (`n` -> facing north, `w` -> facing east), legacy trapdoors, slanted roofing, vines (`wildvine-*-north` -> `north=true`), clay oven/stove/bloomery facing, rails ascending direction.
