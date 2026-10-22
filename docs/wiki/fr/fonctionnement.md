# Fonctionnement

[Accueil du wiki](README.md) · [English](../en/how-it-works.md)

## De Minecraft vers Vintage Story

1. **Lecture.** Les fichiers région (`.mca`, compressés en zlib ou LZ4) sont lus en parallèle. Chaque chunk est décodé selon son format : identifiants numériques (avant 1.13), palettes « aplaties » (1.13 à 1.17), ou disposition 1.18 et plus avec des sections dès Y = -64. Les anciens noms de blocs sont renommés en leurs noms actuels.
2. **Analyse.** Chaque état de bloc distinct est compté, et la table de correspondances lui attribue un bloc Vintage Story, une note et une catégorie. Vos choix (groupes et blocs isolés) s'appliquent par-dessus.
3. **Monde modèle.** OdasiO lance en arrière-plan le serveur dédié de Vintage Story, avec vos réglages (hauteur du monde, type de terrain, climat, saison, mode de jeu), et le laisse créer un monde neuf. On obtient ainsi une sauvegarde que le jeu reconnaît comme la sienne, avec les bons identifiants de blocs pour votre version.
4. **Écriture.** Les chunks Minecraft (colonnes de 16 × 16) sont regroupés en chunks Vintage Story (32 × 32 × 32). Pour chaque bloc : la couche solide, la couche de liquide (eau, lave, blocs immergés), et la block entity si besoin (texte des panneaux, orientation des portes et trappes, type de conteneur, lanterne, terre labourée, ruche, pousse). Le décalage vertical aligne les mers quand le terrain environnant est celui de Vintage Story.
5. **Océan et bords.** Les zones du monde Vintage Story que votre carte ne couvre pas, à l'intérieur de la zone convertie, sont remplies d'océan : la carte ne s'arrête jamais sur du vide.
6. **Éclairage.** La lumière du soleil et celle des blocs (torches, lanternes, pierre lumineuse...) sont calculées par OdasiO avec les mêmes règles que le jeu : le monde est éclairé correctement dès le premier chargement.
7. **Sauvegarde.** Chunks, chunks de carte et données de partie (table des identifiants de blocs, point d'apparition, nom du monde) sont écrits dans le fichier `.vcdbs`, une base SQLite.

## De Vintage Story vers Minecraft

La sauvegarde est lue colonne par colonne, chaque bloc Vintage Story est associé à un état de bloc Minecraft (environ 5 100 règles), les block entities sont retraduites (panneaux, portes, trappes, coffres), et un monde Minecraft Java 1.21.1 est écrit : fichiers région, `level.dat` avec le générateur choisi (océan, normal ou vide) et le mode de jeu.

## L'aperçu 3D

- **Carte du monde.** Une première passe lit toutes les régions et garde, pour chaque colonne, le bloc vu du ciel et son altitude. Elle alimente la vue d'ensemble et le relief lointain.
- **Relief lointain.** Chaque région est dessinée en colonnes colorées, d'autant moins détaillées qu'elle est loin (de 2 à 16 blocs par colonne). Les couleurs viennent des textures du dessus de chaque jeu.
- **Chunks détaillés.** Autour de la caméra, les chunks sont chargés et maillés en tâche de fond : seulement les faces visibles, avec les modèles de chaque jeu (tous les éléments des modèles Minecraft et des blockstates multipart, fichiers de formes de Vintage Story), les textures rangées dans un atlas avec mipmaps.
- **Rendu.** Le GPU dessine les deux mondes (wgpu, via le moteur de rendu d'egui) avec anticrénelage et brouillard de distance ; caméra, sélection et remappage sont partagés entre les deux vues.

## Notes techniques

Pour les contributeurs, les formats sont décrits en détail (en anglais) :

- [Format des sauvegardes Vintage Story](../../vs-save-format.md) : tables SQLite, structure des chunks et chunks de carte, couches de blocs, lumière, clés de position.
- [Block entities de Vintage Story](../../vs-block-entities.md) : le format binaire des attributs et les classes écrites par OdasiO.
- [Format de la table de correspondances](../../mapping.md) : tous les champs de `mapping.toml`, l'expansion des codes de blocs Vintage Story depuis les fichiers du jeu, les conventions de nommage.
