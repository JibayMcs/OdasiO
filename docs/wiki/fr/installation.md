# Installation

[Accueil du wiki](README.md) · [English](../en/installation.md)

## Ce qu'il faut

| | Nécessaire pour | Remarques |
|---|---|---|
| **Vintage Story 1.22** | Convertir vers Vintage Story | OdasiO lance en arrière-plan le serveur dédié du jeu pour créer le nouveau monde avec vos réglages, puis y écrit vos blocs. |
| **Minecraft Java Edition** | Les textures de l'aperçu 3D | Lancez le jeu au moins une fois pour qu'une version soit téléchargée. Inutile pour convertir. |
| Une carte graphique compatible Vulkan, Metal ou OpenGL | L'aperçu 3D | N'importe quelle carte de ces dix dernières années. Le reste de l'application fonctionne sans. |

OdasiO cherche les jeux à leur emplacement par défaut :

| | Linux | Windows |
|---|---|---|
| Vintage Story | `~/.local/share/vintagestory` | `%APPDATA%\Vintagestory` |
| Sauvegardes Vintage Story | `~/.config/VintagestoryData/Saves` | `%APPDATA%\VintagestoryData\Saves` |
| Minecraft | `~/.minecraft` (et le dossier du lanceur Flatpak pour les mondes) | `%APPDATA%\.minecraft` |

Le serveur de Vintage Story a besoin du **runtime .NET** qu'utilise le jeu lui-même : si le jeu se lance, le serveur aussi. Sous Linux, un runtime installé dans `~/.dotnet` est trouvé automatiquement.

## Linux

### Debian, Ubuntu et dérivées

Téléchargez le paquet `.deb` depuis la page [Releases](https://github.com/JibayMcs/OdasiO/releases), puis :

```sh
sudo apt install ./odasio_*.deb
```

Deux programmes sont installés : **OdasiO** (dans le menu des applications, `odasio-gui` depuis un terminal) et la ligne de commande `odasio`.

Pour désinstaller : `sudo apt remove odasio`.

### Autres distributions

Compilez depuis les sources (ci-dessous). L'application est un seul exécutable, sans autre dépendance que vos pilotes graphiques et, pour les fenêtres de choix de fichiers, `xdg-desktop-portal` (installé avec tous les environnements de bureau).

## Windows

Téléchargez l'installateur (`.exe`) depuis la page [Releases](https://github.com/JibayMcs/OdasiO/releases) et lancez-le. OdasiO apparaît dans le menu Démarrer ; la ligne de commande `odasio` est installée à côté.

## Mises à jour

OdasiO cherche une nouvelle version au démarrage (au plus une fois par jour) et affiche un bandeau quand il en trouve une : **Mettre à jour** la télécharge, vérifie son empreinte SHA-256 avec la liste publiée avec la version, et l'installe comme OdasiO a été installé :

- **installateur Windows** : le nouvel installateur s'exécute en silence, puis relance OdasiO ;
- **paquet `.deb`** : installé avec votre mot de passe (`pkexec`) ; à défaut, le paquet s'ouvre dans votre gestionnaire de logiciels ;
- **archive portable** : l'exécutable est remplacé sur place.

Cliquez sur le numéro de version (en haut à droite) pour vérifier tout de suite. La variable d'environnement `ODASIO_NO_UPDATE` désactive la vérification automatique. Les versions compilées en mode debug ne vérifient pas.

## Compiler depuis les sources

1. Installez Rust 1.85 ou plus récent avec [rustup](https://rustup.rs).
2. Sous Linux, installez un compilateur C (pour SQLite et zstd) :

   ```sh
   # Debian, Ubuntu
   sudo apt install build-essential pkg-config
   # Fedora
   sudo dnf install gcc pkgconf-pkg-config
   # Arch
   sudo pacman -S base-devel
   ```

3. Compilez :

   ```sh
   git clone https://github.com/JibayMcs/OdasiO.git
   cd OdasiO
   cargo build --release
   ```

Les programmes sont dans `target/release/` : `odasio-gui` (application) et `odasio` (ligne de commande). Vous pouvez les copier n'importe où, ils embarquent tout ce dont ils ont besoin (correspondances, traductions, icône).

Pour vérifier que tout fonctionne :

```sh
cargo test --workspace
./target/release/odasio info
```
