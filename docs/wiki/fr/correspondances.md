# Correspondances des blocs

[Accueil du wiki](README.md) · [English](../en/block-mapping.md)

Minecraft et Vintage Story n'ont pas les mêmes blocs. OdasiO embarque une table d'environ 2 800 règles de Minecraft vers Vintage Story (et 5 100 pour le retour) qui couvre Minecraft vanilla, des plus anciennes versions aux plus récentes. Chaque état de bloc Minecraft reçoit un bloc Vintage Story, une note et une catégorie.

## Notes

| Note | Signification | Exemple |
|---|---|---|
| **Exact** | Le même bloc. | Planches de chêne, verre, eau |
| **Équivalent** | Même rôle, une variante Vintage Story (essence, couleur, espèce de fleur, roche). | Bûche de bouleau vers bûche de bouleau, laine rouge vers laine rouge, diorite vers marbre blanc |
| **Avec perte** | Le bon bloc, mais une donnée est perdue (mise en forme du texte, orientation stockée autrement). | Panneaux |
| **Remplaçant** | Un autre bloc tient lieu de remplaçant. | Prismarine vers marbre vert, purpur vers marbre rouge poli, observateur |
| **Sans équivalent** | Rien ne convient : de l'air par défaut. | Boutons, plaques de pression, rails, barrières |
| **Non mappé** | Aucune règle : blocs de mods, ou d'un Minecraft plus récent. Les blocs pleins deviennent la roche principale, les autres de l'air, sauf choix contraire. | `create:cogwheel` |

L'étape d'analyse montre la part de chaque note dans votre monde. Sur un monde vanilla typique, plus de 99 % des blocs sont exacts ou équivalents.

## Catégories

Les blocs sont aussi classés par catégorie : terrain, minerais, bois, végétation, liquides, construction, blocs colorés, éclairage, panneaux et bannières, portes et trappes, conteneurs, redstone, décoration, autres. Elles regroupent les choix à l'étape Correspondances.

## Vos choix

Dans l'application (étape 3, et depuis l'aperçu 3D), vous pouvez :

- garder la proposition, remplacer tout un groupe (note et catégorie) par de l'air, ou par n'importe quel bloc Vintage Story ;
- changer un seul état de bloc, ou tous les états d'un bloc (toutes les orientations d'un escalier, par exemple).

Vos choix s'appliquent à la conversion en cours. En ligne de commande, la table utilisée est toujours celle embarquée.

## Roches

Minecraft a une seule pierre ; Vintage Story a une vingtaine de roches. La **roche principale** (granite par défaut) remplace la pierre Minecraft et tout ce qui en est fait : pierre taillée, briques de pierre, gravier, minerais dans la pierre. La diorite, l'andésite et le granite ont leur propre roche Vintage Story. Les roches principales possibles sont celles pour lesquelles tous les blocs dérivés existent dans Vintage Story : andésite, basalte, bauxite, craie, chert, argilite, conglomérat, granite, calcaire, péridotite, phyllade, grès, schiste, ardoise (`andesite`, `basalt`, `bauxite`, `chalk`, `chert`, `claystone`, `conglomerate`, `granite`, `limestone`, `peridotite`, `phyllite`, `sandstone`, `shale`, `slate`).

## Améliorer la table

La table est un simple fichier texte, [`assets/mapping.toml`](../../../assets/mapping.toml), intégré à OdasiO à la compilation. Une règle ressemble à ceci :

```toml
[[block]]
mc = "minecraft:dark_oak_stairs"
when = { half = "bottom", facing = "north" }
vs = "game:plankstairs-walnut-up-north-free"
quality = "exact"
category = "building"
note = "les coins intérieurs et extérieurs deviennent des escaliers droits"
```

- `mc` : l'identifiant du bloc Minecraft ;
- `when` (facultatif) : propriétés d'état qui doivent correspondre ; une liste signifie « l'une de » ;
- `vs` : le code du bloc Vintage Story, `{rock}` désigne la roche principale ;
- `quality` : `exact`, `equivalent`, `lossy` (avec perte), `substitute` (remplaçant), `removed` (sans équivalent) ;
- `category` : `terrain`, `ore`, `wood`, `plant`, `liquid`, `building`, `colored`, `light`, `sign`, `door`, `container`, `redstone`, `decoration` ou `other` ;
- `note` (facultatif) : un commentaire pour le prochain lecteur.

Les autres champs (couche de liquide, block entity) et les conventions de nommage de Vintage Story sont décrits dans les [notes techniques](../../mapping.md) (en anglais).

Les règles sont lues dans l'ordre et la première qui correspond l'emporte : les règles précises passent avant les règles générales. Après modification, recompilez OdasiO (`cargo build --release`) et lancez `odasio plan` sur un monde qui utilise le bloc pour vérifier le résultat.

Un bloc mal converti ? Ouvrez une issue avec le bloc Minecraft (affiché dans l'application quand vous cliquez dessus), ce que vous avez obtenu et ce que vous attendiez, ou proposez une pull request avec la règle corrigée.
