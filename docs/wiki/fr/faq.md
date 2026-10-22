# FAQ et dépannage

[Accueil du wiki](README.md) · [English](../en/faq.md)

### Mon monde Minecraft est-il modifié ?

Non. OdasiO lit seulement le monde d'origine. Le monde Vintage Story est un nouveau fichier ; le monde Minecraft créé par la conversion inverse est un nouveau dossier.

### Quelles versions sont prises en charge ?

- **Minecraft Java Edition** : des versions bêta aux plus récentes, versions 26.x comprises. L'édition Bedrock n'est pas prise en charge.
- **Vintage Story** : 1.22. Le monde converti est créé par le jeu installé, il correspond donc toujours à votre version.

### « Vintage Story introuvable », ou la conversion échoue à la création du monde modèle

OdasiO a besoin du serveur dédié du jeu pour créer le monde. Vérifiez que Vintage Story est installé dans son dossier par défaut (voir [Installation](installation.md#ce-quil-faut)) et que le jeu se lance. S'il est installé ailleurs, vous pouvez convertir en ligne de commande avec `--template`, en donnant une sauvegarde créée par votre jeu.

### L'aperçu n'a pas de textures, ou un côté est vide

L'aperçu lit les textures dans vos installations. Pour Minecraft, lancez le jeu une fois pour qu'une version soit téléchargée dans `.minecraft/versions`. Pour Vintage Story, le jeu doit être dans son dossier par défaut. Un message remplace le côté dont le jeu manque.

### L'aperçu indique que la 3D n'est pas disponible

L'aperçu 3D demande une carte graphique compatible Vulkan, Metal ou OpenGL : mettez à jour vos pilotes graphiques. Le reste de l'application, et la conversion, fonctionnent sans.

### Des zones de mon monde manquent dans Vintage Story

Seuls les chunks sans aucun bloc sont laissés de côté. Les chunks que Minecraft marque comme incomplètement générés (fréquents en bordure de la zone explorée, et dans les mondes retouchés avec des outils comme WorldPainter) sont convertis tels quels. Les blocs hors de la hauteur du monde Vintage Story sont signalés à la fin de la conversion : décochez *Automatique* dans les options et augmentez la hauteur si vous en perdez.

### Mon monde a des zones sombres dans Vintage Story

L'éclairage est calculé par OdasiO pendant la conversion. Si une zone reste sombre, lancez `odasio audit` sur la sauvegarde et joignez la sortie à une issue. En attendant, convertir avec `--lighting game` (superflat uniquement) laisse le jeu le calculer.

### Le contenu des coffres, les tableaux et les créatures sont-ils convertis ?

Pas encore. Les conteneurs sont posés avec leur orientation mais vides, et les entités (créatures, cadres, tableaux, porte-armures) ne sont pas converties. Les panneaux gardent leur texte.

### Les blocs de mods

Les blocs de mods apparaissent comme **Non mappés** à l'étape Correspondances. Par défaut, les blocs pleins deviennent la roche principale et les autres disparaissent ; remplacez-les par le bloc Vintage Story de votre choix, par groupe ou bloc par bloc.

### Combien de temps dure une conversion ?

Sur un ordinateur récent, un monde de 160 000 chunks (environ 11 km de côté) est analysé en 2 secondes environ et converti en moins d'une minute, création du monde modèle comprise.

### Où sont mes réglages ?

La langue et les mondes récents sont dans `~/.config/odasio/` sous Linux et `%APPDATA%\odasio\` sous Windows. Supprimez ce dossier pour repartir de zéro.

### Comment signaler un problème ?

Ouvrez une issue sur [GitHub](https://github.com/JibayMcs/OdasiO/issues) avec votre version d'OdasiO (`odasio info`), la version Minecraft du monde, ce que vous avez fait, ce que vous avez obtenu et ce que vous attendiez. Une capture de l'aperçu 3D, ou les coordonnées affichées quand vous cliquez sur le bloc, aident beaucoup.
