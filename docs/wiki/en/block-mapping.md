# Block mapping

[Wiki home](README.md) · [Français](../fr/correspondances.md)

Minecraft and Vintage Story don't have the same blocks. OdasiO ships a mapping of about 2,800 rules from Minecraft to Vintage Story (and 5,100 for the way back) that covers vanilla Minecraft from the oldest versions to the latest. Each Minecraft block state gets a Vintage Story block, a rating and a category.

## Ratings

| Rating | Meaning | Example |
|---|---|---|
| **Exact** | The same block. | Oak planks, glass, water |
| **Equivalent** | Same role, a Vintage Story variant (wood species, colour, flower species, rock). | Birch log to a birch log, red wool to red wool, diorite to white marble |
| **Lossy** | The right block, but some data is lost (text formatting, an orientation stored differently). | Signs |
| **Substitute** | Another block stands in for it. | Prismarine to green marble, purpur to polished red marble, observer |
| **No equivalent** | Nothing fits; air by default. | Buttons, pressure plates, rails, barriers |
| **Unmapped** | No rule at all: blocks from mods, or from a newer Minecraft. Solid ones become the main rock, the others air, unless you choose otherwise. | `create:cogwheel` |

The analysis step shows the share of each rating in your world. On a typical vanilla world, over 99% of the blocks are exact or equivalent.

## Categories

Blocks are also sorted by category: terrain, ores, wood, vegetation, liquids, building, coloured blocks, lighting, signs and banners, doors and trapdoors, containers, redstone, decoration, other. They group the choices in the Mapping step.

## Your choices

In the app (step 3, and from the 3D preview), you can:

- keep the suggestion, replace a whole group (rating and category) with air, or with any Vintage Story block;
- override a single block state, or every state of a block (all the orientations of a staircase, for instance).

Your choices apply to the conversion you are running. In the command line, the mapping is always the built-in one.

## Rocks

Minecraft has one stone; Vintage Story has about twenty rocks. The **main rock** (granite by default) replaces Minecraft stone and everything built from it: cobblestone, stone bricks, gravel, ores embedded in stone. Diorite, andesite and granite keep their own Vintage Story rock. The available main rocks are the ones for which every derived block exists in Vintage Story: andesite, basalt, bauxite, chalk, chert, claystone, conglomerate, granite, limestone, peridotite, phyllite, sandstone, shale, slate.

## Improving the mapping

The mapping is a plain text file, [`assets/mapping.toml`](../../../assets/mapping.toml), embedded in OdasiO when it is built. A rule looks like this:

```toml
[[block]]
mc = "minecraft:dark_oak_stairs"
when = { half = "bottom", facing = "north" }
vs = "game:plankstairs-walnut-up-north-free"
quality = "exact"
category = "building"
note = "inner and outer corners become straight stairs"
```

- `mc`: the Minecraft block id;
- `when` (optional): blockstate properties that must match; an array means "any of";
- `vs`: the Vintage Story block code, `{rock}` stands for the main rock;
- `quality`: `exact`, `equivalent`, `lossy`, `substitute`, `removed` (no equivalent);
- `category`: `terrain`, `ore`, `wood`, `plant`, `liquid`, `building`, `colored`, `light`, `sign`, `door`, `container`, `redstone`, `decoration` or `other`;
- `note` (optional): a comment shown nowhere but useful to the next reader.

Other fields (liquid layer, block entity) and the naming conventions of Vintage Story are described in the [technical notes](../../mapping.md).

Rules are read in order and the first match wins, so specific rules come before general ones. After editing, rebuild OdasiO (`cargo build --release`) and run `odasio plan` on a world that uses the block to check the result.

Found a block that converts badly? Open an issue with the Minecraft block (shown in the app when you click it), what you got and what you expected, or send a pull request with the fixed rule.
