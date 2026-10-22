# Command line

[Wiki home](README.md) · [Français](../fr/cli.md)

The `odasio` command does everything the app does, plus a few diagnostic tools. Every command has a `--help`.

> The command line's help and messages are in French for now; the options below are the same in every language.

```text
odasio info          Version and supported formats
odasio scan          Inventory of the blocks of a Minecraft world
odasio plan          What each block of the world will become
odasio convert       Convert a Minecraft dimension to a Vintage Story world
odasio to-minecraft  Convert a Vintage Story save to a Minecraft Java world
odasio inspect       Inspect one column of a Vintage Story save
odasio audit         Check the generation passes and lighting of a Vintage Story save
```

## scan

```sh
odasio scan <WORLD> [--dimension minecraft:overworld] [--top 40] [--regions N] [--out inventory.tsv]
```

Reads the whole world and lists the most common block states. `--out` writes the full inventory (count and block state, tab separated). `--regions` limits the number of region files read, handy for a quick look at a huge world.

## plan

```sh
odasio plan <WORLD> [--dimension ...] [--rock granite] [--top 8] [--regions N]
```

Shows what every block of the world will become, grouped by rating, with the number of blocks concerned. `--rock` previews another main rock.

## convert

```sh
odasio convert <WORLD> [OPTIONS]
```

| Option | Default | Meaning |
|---|---|---|
| `--name <NAME>` | `Odasio` | World name in Vintage Story, also used for the file name |
| `--out <FILE>` | Vintage Story's saves folder | `.vcdbs` file to write |
| `--force` | | Overwrite the save if it exists |
| `--dimension <ID>` | `minecraft:overworld` | Dimension to convert |
| `--rock <ROCK>` | `granite` | Vintage Story rock replacing Minecraft stone |
| `--world-type <TYPE>` | `superflat` | Terrain around the map: `superflat`, `standard` (Vintage Story terrain, sea levels aligned) or `ocean` |
| `--height <N>` | automatic | World height (multiple of 32). 384 in superflat; in standard and ocean, the smallest height that contains the world |
| `--y-offset <N>` | automatic | Added to Minecraft Y. 64 in superflat; in standard and ocean, aligns the seas |
| `--season <SEASON>` | `cycle` | `cycle`, `spring`, `summer`, `fall` or `winter` |
| `--game-mode <MODE>` | `creative` | `creative` or `survival` |
| `--lighting <WHO>` | `odasio` | `odasio` computes the lighting; `game` lets the game do it on first load (superflat only) |
| `--template <FILE>` | | Use an existing Vintage Story save as template instead of generating one with the server |

Examples:

```sh
# Recommended: Vintage Story terrain around the map, sea levels aligned
odasio convert ~/.minecraft/saves/Kingdom --name "Kingdom" --world-type standard

# An island in an endless ocean, always summer, survival
odasio convert ~/.minecraft/saves/Kingdom --name "Kingdom island" \
  --world-type ocean --season summer --game-mode survival

# The Nether, with basalt as the main rock
odasio convert ~/.minecraft/saves/Kingdom --dimension minecraft:the_nether \
  --name "Kingdom Nether" --rock basalt
```

## to-minecraft

```sh
odasio to-minecraft <SAVE.vcdbs> <WORLD_FOLDER> [OPTIONS]
```

| Option | Default | Meaning |
|---|---|---|
| `--name <NAME>` | `OdasiO` | World name in Minecraft |
| `--force` | | Replace the folder if it exists |
| `--terrain <TYPE>` | `ocean` | Outside the converted area: `ocean`, `normal` (Minecraft terrain) or `void` |
| `--game-mode <MODE>` | `creative` | `creative` or `survival` |

The world is written in the Minecraft Java 1.21.1 format.

```sh
odasio to-minecraft ~/.config/VintagestoryData/Saves/Kingdom.vcdbs \
  ~/.minecraft/saves/Kingdom-back --name "Kingdom (back)"
```

## inspect and audit

Diagnostic tools for Vintage Story saves:

```sh
odasio inspect <SAVE.vcdbs> <X> <Z>   # blocks, fluids and light of the column at X, Z
odasio audit <SAVE.vcdbs>             # generation passes and lighting state of every column
```

Useful when a converted world shows dark areas or regenerated terrain: attach their output to your bug report.

## Exit codes and logs

`odasio` returns 0 on success and 1 on error, with the message on standard error. Set `RUST_LOG=debug` for detailed logs.
