# How it works

[Wiki home](README.md) · [Français](../fr/fonctionnement.md)

## Minecraft to Vintage Story

1. **Reading.** Region files (`.mca`, zlib or LZ4 compressed) are read in parallel. Each chunk is decoded according to its format: numeric ids (before 1.13), flattened palettes (1.13 to 1.17), or the 1.18+ layout with sections from Y = -64. Old block names are renamed to their current ones.
2. **Analysis.** Every distinct block state is counted, and the mapping gives each one a Vintage Story block, a rating and a category. Your choices (groups and single blocks) are applied on top.
3. **Template world.** OdasiO starts the Vintage Story dedicated server in the background, with your settings (world height, terrain type, climate, season, game mode), and lets it create a fresh world. This guarantees a save the game accepts as its own, with the right block ids for your version.
4. **Writing.** Minecraft chunks (16 × 16 columns) are packed into Vintage Story chunks (32 × 32 × 32). For each block: the solid layer, the fluid layer (water, lava, waterlogged blocks), and the block entity when needed (sign text, door and trapdoor orientation, container type, lantern, farmland, beehive, sapling). The vertical offset aligns the seas when the surrounding terrain is Vintage Story's.
5. **Ocean and edges.** Areas of the Vintage Story world that your map doesn't cover inside the converted zone are filled with ocean, so the map never ends on a void.
6. **Lighting.** Sunlight and block light (torches, lanterns, glowstone...) are computed by OdasiO with the same rules as the game, so the world is lit correctly on first load.
7. **Saving.** Chunks, map chunks and the game data (block id table, spawn point, world name) are written to the `.vcdbs` file, a SQLite database.

## Vintage Story to Minecraft

The save is read column by column, each Vintage Story block is matched to a Minecraft block state (about 5,100 rules), block entities are translated back (signs, doors, trapdoors, chests), and a Minecraft Java 1.21.1 world is written: region files, `level.dat` with the chosen generator (ocean, normal or void) and game mode.

## The 3D preview

- **World map.** A first pass reads every region and keeps, for each column, the block seen from the sky and its height. It feeds the overview and the distant terrain.
- **Distant terrain.** Each region is drawn as coloured columns, with fewer details the farther it is (2 to 16 blocks per column). Colours come from the top textures of each game.
- **Detailed chunks.** Around the camera, chunks are loaded and meshed in the background: only visible faces, with the models of each game (all elements of Minecraft models and multipart blockstates, Vintage Story shape files), the textures packed in an atlas with mipmaps.
- **Rendering.** The GPU draws both worlds (wgpu, through the egui renderer) with multisampling and distance fog; the camera, picking and remapping are shared between the two views.

## Technical notes

For contributors, the formats are documented in detail:

- [Vintage Story save format](../../vs-save-format.md): SQLite tables, chunk and map chunk layouts, block layers, light, position keys.
- [Vintage Story block entities](../../vs-block-entities.md): the binary tree attribute format and the classes OdasiO writes.
- [Mapping file format](../../mapping.md): every field of `mapping.toml`, how Vintage Story block codes are expanded from the game's files, and the naming conventions.
