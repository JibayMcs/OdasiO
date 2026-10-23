# User guide

[Wiki home](README.md) · [Français](../fr/guide.md)

OdasiO guides you through six steps, shown at the top of the window. You can go back to any finished step by clicking it. The language (FR / EN) and the theme (light / dark) are switched from the top right corner.

## 1. World

![Choose a world](../../images/en-world.png)

Pick the conversion direction at the top: **Minecraft › Vintage Story** (the default) or **Vintage Story › Minecraft** (see [below](#converting-vintage-story-to-minecraft)).

Your recent worlds and the worlds of your Minecraft installation are listed as cards, with their version. **Browse...** opens any world folder (the one that contains `level.dat`). A *modded* badge means the world was played with mods: their blocks are listed as *unmapped* and you choose what to do with them.

Your world is only read. OdasiO never writes into it.

## 2. Analysis

![World analysis](../../images/en-analysis.png)

OdasiO reads every chunk of the world (about 2 seconds for 160,000 chunks) and shows:

- the number of regions, chunks, distinct block states and blocks;
- the **conversion fidelity**: the share of blocks that convert exactly, to an equivalent, with a loss, to a substitute, to nothing, or that have no rule yet ([details](block-mapping.md#ratings));
- the most common blocks.

The **Dimension** menu lets you convert the Nether, the End or a modded dimension instead of the Overworld.

## 3. Mapping

![Block mapping](../../images/en-mapping.png)

Only blocks present in your world are listed, grouped by rating and category. The groups that deserve a look (lossy, substitute, no equivalent, unmapped) come first; tick **Also show exact and equivalent blocks** to see everything.

For each group, the menu on the right decides the fate of all its blocks:

- **Keep suggestion**: use the mapping's proposal;
- **Replace with air**: remove these blocks;
- **Replace with a block...**: pick any Vintage Story block from the searchable list.

Open **Block details** to act on a single block state: **Change** opens the same picker, either for this exact state or for every state of this block (the checkbox in the picker). **Reset** removes your override.

The search field filters by Minecraft or Vintage Story name.

## 4. Preview

![3D preview](../../images/en-preview-island.jpg)

Both worlds are shown side by side in 3D, Minecraft on the left and Vintage Story on the right, with one shared camera. Textures and models come from your game installations.

| Action | Mouse | Buttons |
|---|---|---|
| Move | Drag | |
| Zoom toward the cursor | Wheel, or double click to zoom in | `+` `-` |
| Rotate and tilt | Right-click drag, or Shift + drag | `‹` `›` rotate by 45° |
| North on top | Click the compass | |
| Top view / tilted view | | `2D` / `3D` |
| Whole world | | the frame icon |

When you arrive, the whole world is visible from above. As you zoom in, the view tilts by itself (top view far away, oblique view up close) until you tilt it yourself. Far away, the terrain comes from a map of the world; up close, the chunks around the camera are loaded in the background and drawn block by block, with detailed shapes. A spinner next to the titles means chunks are still loading.

**Click a block** to select it. The bar at the bottom shows its position, the Minecraft block, its rating and the Vintage Story block it becomes. **Change Vintage Story block** opens the picker: the Vintage Story side is rebuilt with your choice right away. **Deselect** clears the selection.

The Vintage Story side shows the ocean that OdasiO fills around your map, so you see exactly where the world ends.

### Choosing the area to convert

To convert only part of the world (a city, a build, a few islands), click **Choose the area to convert** at the bottom right. The chunk grid appears on the terrain:

- **drag** to add a rectangle of chunks, **Alt + drag** to remove one, **click** to add or remove a single chunk;
- the **middle button** moves the view, the wheel zooms and the right button rotates, as usual.

The selected chunks are tinted and everything else is darkened, on both views. The bar shows the number of chunks and the size of the area. **Done selecting** goes back to normal navigation; **Clear all** empties the selection. Only these chunks are converted, and the Vintage Story world is centred on them. Without a selection, the whole world is converted.

## 5. Options

![World options](../../images/en-options.png)

| Option | Meaning |
|---|---|
| **World name** | The name in Vintage Story's world list. The save file is named after it. |
| **Save file** | Where the `.vcdbs` file is written; by default, Vintage Story's saves folder. **Choose...** picks another place. |
| **Main rock** | The Vintage Story rock that replaces Minecraft stone. Ores and stone variants follow it where the game allows. |
| **Surrounding terrain** | What Vintage Story generates around your map. **Standard**: the game's terrain, climate, seasons and vegetation, with the sea levels aligned (recommended). **Ocean**: water as far as the eye can see, your world becomes an island. **Superflat**: a flat plain. |
| **Area to convert** | The whole world, or the chunks chosen in the preview. **Choose in the preview** opens the selection, **Convert everything** clears it. |
| **Season** | Seasons enabled, or locked on spring, summer, fall or winter. Can be changed later in game with `/worldconfig seasons`. |
| **Game mode** | Creative or Survival. |
| **Height and level** | *Automatic* picks the smallest world height that contains your world and aligns the Minecraft sea (Y = 63) with the Vintage Story sea. Untick it to set the height (multiple of 32) and the vertical offset yourself. |

## 6. Convert

The summary recalls the destination, the settings and your mapping choices. **Convert** starts the conversion: the Vintage Story server first creates the template world (a few seconds), then OdasiO writes your blocks, block entities and lighting.

When it's done, open Vintage Story, go to **Singleplayer**: the new world is in the list. **Open saves folder** shows the file.

The report lists the converted columns, the skipped chunks, the blocks outside the height range and the blocks the game doesn't know (replaced with air).

## Converting Vintage Story to Minecraft

Choose **Vintage Story › Minecraft** on the first step. The steps are the same:

1. **World**: pick a `.vcdbs` save (the saves of your Vintage Story installation are listed).
2. **Analysis**: columns, block types and fidelity toward Minecraft.
3. **Mapping**: the blocks of your save and their Minecraft equivalents.
4. **Options**: world name, folder (your Minecraft saves by default), terrain outside the converted area (**Ocean**, **Normal Minecraft terrain** or **Void**) and game mode.
5. **Convert**: the world is written in the Minecraft Java 1.21.1 format; newer versions upgrade it automatically when you open it.

Signs keep their text, doors, trapdoors and chests keep their orientation. Container contents are not converted yet.
