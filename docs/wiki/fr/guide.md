# Guide d'utilisation

[Accueil du wiki](README.md) · [English](../en/user-guide.md)

OdasiO vous guide en six étapes, affichées en haut de la fenêtre. Vous pouvez revenir à toute étape terminée en cliquant dessus. La langue (FR / EN) et le thème (clair / sombre) se changent en haut à droite.

## 1. Monde

![Choix du monde](../../images/fr-world.png)

Choisissez le sens de conversion en haut : **Minecraft › Vintage Story** (par défaut) ou **Vintage Story › Minecraft** (voir [plus bas](#convertir-vintage-story-vers-minecraft)).

Vos mondes récents et ceux de votre installation de Minecraft sont présentés en cartes, avec leur version. **Parcourir...** ouvre n'importe quel dossier de monde (celui qui contient `level.dat`). Le badge *moddé* signale un monde joué avec des mods : leurs blocs apparaîtront comme *non mappés* et vous choisirez quoi en faire.

Votre monde est seulement lu : OdasiO n'y écrit jamais.

## 2. Analyse

![Analyse du monde](../../images/fr-analysis.png)

OdasiO lit tous les chunks du monde (2 secondes environ pour 160 000 chunks) et affiche :

- le nombre de régions, de chunks, d'états de blocs distincts et de blocs ;
- la **fidélité de la conversion** : la part des blocs convertis à l'identique, en équivalent, avec perte, en remplaçant, en rien, ou encore sans règle ([détails](correspondances.md#notes)) ;
- les blocs les plus courants.

Le menu **Dimension** permet de convertir le Nether, l'End ou une dimension de mod au lieu de l'Overworld.

## 3. Correspondances

![Correspondances](../../images/fr-mapping.png)

Seuls les blocs présents dans votre monde sont listés, regroupés par note et par catégorie. Les groupes à vérifier (avec perte, remplaçants, sans équivalent, non mappés) passent en premier ; cochez **Afficher aussi les blocs exacts et équivalents** pour tout voir.

Pour chaque groupe, le menu de droite décide du sort de tous ses blocs :

- **Garder la proposition** : utiliser la correspondance prévue ;
- **Remplacer par de l'air** : supprimer ces blocs ;
- **Remplacer par un bloc...** : choisir n'importe quel bloc Vintage Story dans la liste, avec recherche.

Ouvrez **Détail des blocs** pour agir sur un seul état : **Changer** ouvre le même sélecteur, pour cet état précis ou pour tous les états du bloc (case à cocher du sélecteur). **Annuler** retire votre choix.

Le champ de recherche filtre par nom Minecraft ou Vintage Story.

## 4. Aperçu

![Aperçu 3D](../../images/fr-preview-island.jpg)

Les deux mondes sont affichés côte à côte en 3D, Minecraft à gauche et Vintage Story à droite, avec une seule caméra partagée. Textures et modèles viennent de vos installations des jeux.

| Action | Souris | Boutons |
|---|---|---|
| Se déplacer | Glisser | |
| Zoomer vers le curseur | Molette, ou double-clic pour zoomer | `+` `-` |
| Tourner et incliner | Clic droit glissé, ou Maj + glisser | `‹` `›` tournent de 45° |
| Nord en haut | Clic sur la boussole | |
| Vue du dessus / vue inclinée | | `2D` / `3D` |
| Tout le monde | | l'icône de cadre |

En arrivant, tout le monde est visible du dessus. En zoomant, la vue s'incline d'elle-même (du dessus de loin, plongeante de près) jusqu'à ce que vous l'inclinez vous-même. De loin, le relief vient d'une carte du monde ; de près, les chunks autour de la caméra se chargent en tâche de fond et s'affichent bloc par bloc, avec leurs formes détaillées. Un indicateur tournant près des titres signale un chargement en cours.

**Cliquez sur un bloc** pour le sélectionner. La barre du bas affiche sa position, le bloc Minecraft, sa note et le bloc Vintage Story qu'il devient. **Changer le bloc Vintage Story** ouvre le sélecteur : le côté Vintage Story est reconstruit aussitôt avec votre choix. **Désélectionner** efface la sélection.

Le côté Vintage Story montre l'océan qu'OdasiO ajoute autour de votre carte : vous voyez exactement où le monde s'arrête.

## 5. Options

![Options du monde](../../images/fr-options.png)

| Option | Rôle |
|---|---|
| **Nom du monde** | Le nom dans la liste des mondes de Vintage Story ; le fichier de sauvegarde porte ce nom. |
| **Fichier de sauvegarde** | Où le fichier `.vcdbs` est écrit ; par défaut, le dossier des sauvegardes de Vintage Story. **Choisir...** en désigne un autre. |
| **Roche principale** | La roche Vintage Story qui remplace la pierre Minecraft. Minerais et variantes de pierre suivent, quand le jeu le permet. |
| **Terrain environnant** | Ce que Vintage Story génère autour de votre carte. **Standard** : relief, climat, saisons et végétation du jeu, niveaux de la mer alignés (recommandé). **Océan** : de l'eau à perte de vue, votre monde devient une île. **Superflat** : une plaine plate. |
| **Saison** | Saisons actives, ou bloquées sur le printemps, l'été, l'automne ou l'hiver. Modifiable ensuite en jeu avec `/worldconfig seasons`. |
| **Mode de jeu** | Créatif ou Survie. |
| **Hauteur et niveau** | *Automatique* choisit la plus petite hauteur de monde qui contient le vôtre et aligne la mer de Minecraft (Y = 63) sur celle de Vintage Story. Décochez pour régler vous-même la hauteur (multiple de 32) et le décalage vertical. |

## 6. Conversion

Le récapitulatif rappelle la destination, les réglages et vos choix de correspondances. **Convertir** lance la conversion : le serveur de Vintage Story crée d'abord le monde modèle (quelques secondes), puis OdasiO écrit vos blocs, les block entities et l'éclairage.

Une fois terminé, ouvrez Vintage Story, puis **Solo** : le nouveau monde est dans la liste. **Ouvrir le dossier des sauvegardes** montre le fichier.

Le compte rendu indique les colonnes converties, les chunks ignorés, les blocs hors de la hauteur du monde et les blocs inconnus du jeu (remplacés par de l'air).

## Convertir Vintage Story vers Minecraft

Choisissez **Vintage Story › Minecraft** à la première étape. Les étapes sont les mêmes :

1. **Monde** : choisissez une sauvegarde `.vcdbs` (celles de votre installation de Vintage Story sont listées).
2. **Analyse** : colonnes, types de blocs et fidélité vers Minecraft.
3. **Correspondances** : les blocs de votre sauvegarde et leurs équivalents Minecraft.
4. **Options** : nom du monde, dossier (vos mondes Minecraft par défaut), terrain hors de la zone convertie (**Océan**, **Terrain Minecraft normal** ou **Vide**) et mode de jeu.
5. **Conversion** : le monde est écrit au format Minecraft Java 1.21.1 ; les versions plus récentes le mettent à jour automatiquement à l'ouverture.

Les panneaux gardent leur texte, les portes, trappes et coffres leur orientation. Le contenu des coffres n'est pas encore converti.
