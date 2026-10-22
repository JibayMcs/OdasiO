# FAQ and troubleshooting

[Wiki home](README.md) · [Français](../fr/faq.md)

### Is my Minecraft world modified?

No. OdasiO only reads the source world. The Vintage Story world is a new file; the Minecraft world created by the reverse conversion is a new folder.

### Which versions are supported?

- **Minecraft Java Edition**: from the beta versions to the latest releases, including the 26.x versions. Bedrock Edition is not supported.
- **Vintage Story**: 1.22. The converted world is created by your installed game, so it always matches your version.

### "Vintage Story not found" or the conversion fails while creating the template world

OdasiO needs the game's dedicated server to create the world. Check that Vintage Story is installed in its default folder (see [Installation](installation.md#what-you-need)) and that the game starts. If you installed it elsewhere, you can convert from the command line with `--template`, giving a save created by your game.

### The preview has no textures, or one side is empty

The preview reads the textures from your installations. For Minecraft, launch the game once so a version is downloaded into `.minecraft/versions`. For Vintage Story, the game must be in its default folder. A message replaces the side whose game is missing.

### The preview says 3D is not available

The 3D preview needs a graphics card with Vulkan, Metal or OpenGL. Update your graphics drivers. The rest of the app, and the conversion, work without it.

### Some areas of my world are missing in Vintage Story

Only chunks without any block are left out. Chunks that Minecraft marks as not fully generated (common at the edge of the explored area, and in worlds edited with tools such as WorldPainter) are converted as they are. Blocks outside the height range of the Vintage Story world are reported at the end of the conversion: untick *Automatic* in the options and raise the height if you lose some.

### My world shows dark areas in Vintage Story

Lighting is computed by OdasiO during the conversion. If an area stays dark, run `odasio audit` on the save and attach the output to an issue. As a workaround, converting with `--lighting game` (superflat only) lets the game compute it.

### Are chest contents, paintings and mobs converted?

Not yet. Containers are placed with their orientation but empty, and entities (mobs, item frames, paintings, armour stands) are not converted. Signs keep their text.

### Modded blocks

Blocks from mods appear as **Unmapped** in the Mapping step. By default, solid blocks become the main rock and the others disappear; replace them with any Vintage Story block you like, group by group or block by block.

### How long does a conversion take?

On a recent computer, a world of 160,000 chunks (about 11 km across) is analysed in about 2 seconds and converted in under a minute, including the creation of the template world.

### Where are my settings stored?

The language and the recent worlds are in `~/.config/odasio/` on Linux and `%APPDATA%\odasio\` on Windows. Delete this folder to start fresh.

### How do I report a problem?

Open an issue on [GitHub](https://github.com/JibayMcs/OdasiO/issues) with your OdasiO version (`odasio info`), the Minecraft version of the world, what you did, what you got and what you expected. A screenshot of the 3D preview, or the coordinates shown when you click the block, helps a lot.
