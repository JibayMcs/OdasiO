# Odasio

Convertisseur de mondes **Minecraft Java Edition** (1.18+) vers **Vintage Story** (1.22.7), avec interface graphique et table de correspondance des blocs pré-remplie et éditable.

## Structure

| Crate | Rôle |
|---|---|
| `odasio-mc` | Lecture des mondes Minecraft (`level.dat`, régions Anvil, NBT) |
| `odasio-vs` | Lecture / écriture des sauvegardes Vintage Story (`.vcdbs`, SQLite) |
| `odasio-core` | Mapping des blocs et pipeline de conversion |
| `odasio-gui` | Interface graphique (eframe / egui) |
| `odasio-cli` | Ligne de commande (`odasio`) |

## Ressources

- `assets/mapping.toml` : correspondances Minecraft vers Vintage Story
- `assets/vs_block_codes.txt` : inventaire des codes de blocs VS 1.22.7
- `docs/vs-save-format.md` : format de sauvegarde VS (obtenu par décompilation)

## Commandes

```sh
cargo run -p odasio-cli -- info
cargo run -p odasio-gui
```
