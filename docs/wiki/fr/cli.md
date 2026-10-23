# Ligne de commande

[Accueil du wiki](README.md) · [English](../en/cli.md)

La commande `odasio` fait tout ce que fait l'application, plus quelques outils de diagnostic. Chaque commande a son `--help`.

```text
odasio info          Version et formats pris en charge
odasio scan          Inventaire des blocs d'un monde Minecraft
odasio plan          Ce que deviendra chaque bloc du monde
odasio convert       Convertit une dimension Minecraft en monde Vintage Story
odasio to-minecraft  Convertit une sauvegarde Vintage Story en monde Minecraft Java
odasio inspect       Inspecte une colonne d'une sauvegarde Vintage Story
odasio audit         Vérifie les passes de génération et l'éclairage d'une sauvegarde Vintage Story
```

## scan

```sh
odasio scan <MONDE> [--dimension minecraft:overworld] [--top 40] [--regions N] [--out inventaire.tsv]
```

Lit tout le monde et liste les états de blocs les plus courants. `--out` écrit l'inventaire complet (nombre et état de bloc, séparés par une tabulation). `--regions` limite le nombre de fichiers région lus, pratique pour un coup d'œil rapide sur un très grand monde.

## plan

```sh
odasio plan <MONDE> [--dimension ...] [--rock granite] [--top 8] [--regions N]
```

Montre ce que deviendra chaque bloc du monde, regroupé par note, avec le nombre de blocs concernés. `--rock` essaie une autre roche principale.

## convert

```sh
odasio convert <MONDE> [OPTIONS]
```

| Option | Par défaut | Rôle |
|---|---|---|
| `--name <NOM>` | `Odasio` | Nom du monde dans Vintage Story, aussi utilisé pour le nom du fichier |
| `--out <FICHIER>` | dossier des sauvegardes de Vintage Story | Fichier `.vcdbs` à écrire |
| `--force` | | Écrase la sauvegarde si elle existe |
| `--dimension <ID>` | `minecraft:overworld` | Dimension à convertir |
| `--rock <ROCHE>` | `granite` | Roche Vintage Story qui remplace la pierre Minecraft |
| `--world-type <TYPE>` | `superflat` | Terrain autour de la carte : `superflat`, `standard` (terrain de Vintage Story, mers alignées) ou `ocean` |
| `--height <N>` | automatique | Hauteur du monde (multiple de 32). 384 en superflat ; en standard et en océan, la plus petite hauteur qui contient le monde |
| `--y-offset <N>` | automatique | Ajouté au Y Minecraft. 64 en superflat ; en standard et en océan, aligne les mers |
| `--season <SAISON>` | `cycle` | `cycle`, `spring`, `summer`, `fall` ou `winter` |
| `--game-mode <MODE>` | `creative` | `creative` ou `survival` |
| `--lighting <QUI>` | `odasio` | `odasio` calcule l'éclairage ; `game` laisse le jeu le faire au premier chargement (superflat uniquement) |
| `--area <X1,Z1,X2,Z2>` | tout le monde | Ne convertit que cette zone, en coordonnées de blocs (arrondie aux chunks de 16 blocs) ; répétable pour plusieurs zones. Le monde est centré sur elles |
| `--template <FICHIER>` | | Utilise une sauvegarde Vintage Story existante comme modèle au lieu d'en générer une avec le serveur |

Exemples :

```sh
# Recommandé : terrain de Vintage Story autour de la carte, mers alignées
odasio convert ~/.minecraft/saves/Royaume --name "Royaume" --world-type standard

# Une île dans un océan sans fin, toujours l'été, en survie
odasio convert ~/.minecraft/saves/Royaume --name "Royaume île" \
  --world-type ocean --season summer --game-mode survival

# Seulement une ville, en île
odasio convert ~/.minecraft/saves/Royaume --name "Royaume ville" \
  --world-type ocean --area 2400,2050,2655,2305

# Le Nether, avec du basalte comme roche principale
odasio convert ~/.minecraft/saves/Royaume --dimension minecraft:the_nether \
  --name "Royaume Nether" --rock basalt
```

## to-minecraft

```sh
odasio to-minecraft <SAUVEGARDE.vcdbs> <DOSSIER_DU_MONDE> [OPTIONS]
```

| Option | Par défaut | Rôle |
|---|---|---|
| `--name <NOM>` | `OdasiO` | Nom du monde dans Minecraft |
| `--force` | | Remplace le dossier s'il existe |
| `--terrain <TYPE>` | `ocean` | Hors de la zone convertie : `ocean`, `normal` (terrain Minecraft) ou `void` (vide) |
| `--game-mode <MODE>` | `creative` | `creative` ou `survival` |

Le monde est écrit au format Minecraft Java 1.21.1.

```sh
odasio to-minecraft ~/.config/VintagestoryData/Saves/Royaume.vcdbs \
  ~/.minecraft/saves/Royaume-retour --name "Royaume (retour)"
```

## inspect et audit

Outils de diagnostic des sauvegardes Vintage Story :

```sh
odasio inspect <SAUVEGARDE.vcdbs> <X> <Z>   # blocs, fluides et lumière de la colonne en X, Z
odasio audit <SAUVEGARDE.vcdbs>             # passes de génération et état de l'éclairage de chaque colonne
```

Utiles quand un monde converti montre des zones sombres ou du terrain régénéré : joignez leur sortie à votre signalement.

## Codes de retour et journaux

`odasio` renvoie 0 en cas de succès et 1 en cas d'erreur, avec le message sur la sortie d'erreur. `RUST_LOG=debug` affiche des journaux détaillés.
