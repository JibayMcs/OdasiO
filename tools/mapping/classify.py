"""Reclasse assets/mapping.toml : remplace `approx = true` par `quality` et ajoute `category`.

Usage : python3 -I tools/mapping/classify.py assets/mapping.toml

quality :
  exact       même bloc
  equivalent  même rôle, variante VS (essence, couleur, espèce, coins d'escalier)
  lossy       bon bloc mais une donnée est perdue (texte, contenu, orientation en block entity)
  substitute  un autre bloc tient lieu de remplaçant
  removed     aucun équivalent, air
Idempotent : les règles ayant déjà un `quality` ne sont pas modifiées.
"""

import re
import sys

LOSSY_HINTS = ("content", "text", "block entity", "orientation", "facing", "rotation",
               "hinge", "unverified", "lower block", "material lives", "not mapped")
SUBSTITUTE_HINTS = ("no vs equivalent", "placeholder", "approximated by", "-> barrel",
                    "redstone -> cinnabar", "no crafting station", "no giant mushrooms",
                    "no metal")
# Remplaçants manifestes, même sans note explicative.
SUBSTITUTE_NAMES = re.compile(
    r"glowstone|sea_lantern|shroomlight|froglight|beacon|prismarine|purpur|netherite_block"
    r"|lapis_block|amethyst_block|sculk|soul_soil|soul_sand|redstone|copper_bulb|resin"
    r"|melon|ancient_debris|crying_obsidian|end_stone|end_rod|chorus|honey|slime|target"
    r"|respawn_anchor|lodestone|conduit|bell|jukebox|note_block|observer|piston|hopper"
    r"|dropper|dispenser|crafter|daylight|tnt|spawner|trial|vault|heavy_core|sniffer_egg"
    r"|dragon|turtle_egg|frogspawn|nether_wart|warped_wart|wart_block|magma|blackstone"
    r"|basalt|nylium|fungus|roots|weeping|twisting|hyphae|crimson|warped|cauldron|scaffolding"
)

CATEGORIES = [
    ("liquid", r"^(water|lava|bubble_column)$"),
    ("redstone", r"redstone|repeater|comparator|piston|observer|lever|button|pressure_plate"
                 r"|daylight|target|tnt|hopper|dropper|dispenser|crafter|rail|tripwire"
                 r"|note_block|copper_bulb|lightning_rod|sculk_sensor|trapped_chest"),
    ("ore", r"_ore$|ancient_debris|^raw_.*_block$"),
    ("container", r"chest|barrel|shulker|furnace|smoker|blast_furnace|brewing|hopper"
                  r"|cauldron|decorated_pot|chiseled_bookshelf|lectern|crafter"),
    ("sign", r"sign|banner"),
    ("door", r"door|trapdoor|fence_gate"),
    ("light", r"torch|lantern|glowstone|lamp|froglight|shroomlight|candle|campfire"
              r"|sea_lantern|beacon|end_rod|jack_o_lantern"),
    ("colored", r"^(white|orange|magenta|light_blue|yellow|lime|pink|gray|light_gray|cyan"
                r"|purple|blue|brown|green|red|black)_"),
    ("wood", r"_log$|_wood$|_stem$|hyphae|planks|leaves|sapling|bamboo|_fence$"),
    ("plant", r"grass|fern|flower|tulip|poppy|dandelion|orchid|allium|bluet|daisy|cornflower"
              r"|lily|rose|lilac|peony|sunflower|bush|vine|moss|lichen|kelp|seagrass|coral"
              r"|mushroom|fungus|roots|cactus|sugar_cane|crop|wheat|carrots|potatoes"
              r"|beetroots|melon|pumpkin|stem|dripleaf|azalea|spore|petals|eyeblossom|wart"
              r"|propagule|pickle|cocoa|berries|chorus|leaf_litter|dead_bush"),
    ("terrain", r"^(stone|granite|diorite|andesite|deepslate|tuff|calcite|dirt|coarse_dirt"
                r"|rooted_dirt|podzol|mycelium|grass_block|sand|red_sand|gravel|clay|mud"
                r"|snow|snow_block|ice|packed_ice|blue_ice|bedrock|obsidian|netherrack"
                r"|end_stone|dripstone_block|pointed_dripstone|sandstone|red_sandstone"
                r"|terracotta|basalt|blackstone|magma_block|soul_sand|soul_soil|powder_snow"
                r"|farmland|dirt_path|infested_.*)$"),
    ("building", r"brick|stairs|slab|wall|polished|smooth|cut_|chiseled|pillar|tiles|glass"
                 r"|concrete|copper|quartz|purpur|prismarine|block$"),
]


def classify_category(name):
    for category, pattern in CATEGORIES:
        if re.search(pattern, name):
            return category
    return "decoration"


def classify_quality(body, vs, note):
    if vs == "game:air":
        return "removed"
    if "approx = true" not in body:
        return "exact"
    low = note.lower()
    if any(h in low for h in SUBSTITUTE_HINTS):
        return "substitute"
    if any(h in low for h in LOSSY_HINTS):
        return "lossy"
    return None  # décidé selon le nom


def main(path):
    text = open(path, encoding="utf-8").read()
    head, *rules = re.split(r"^\[\[block\]\]\n", text, flags=re.M)
    out = [head]
    stats = {}
    for body in rules:
        if re.search(r"^quality = ", body, re.M):
            out.append("[[block]]\n" + body)
            continue
        get = lambda k: (re.search(r'^%s = "([^"]*)"' % k, body, re.M) or [None, ""])[1]
        mc, vs, note = get("mc"), get("vs"), get("note")
        name = mc.split(":", 1)[1]
        quality = classify_quality(body, vs, note)
        if quality is None:
            quality = "substitute" if SUBSTITUTE_NAMES.search(name) else "equivalent"
        category = classify_category(name)
        stats[quality] = stats.get(quality, 0) + 1
        body = re.sub(r"^approx = true\n", "", body, flags=re.M)
        body = re.sub(r'^(vs = "[^"]*"\n)',
                      r'\1quality = "%s"\ncategory = "%s"\n' % (quality, category),
                      body, count=1, flags=re.M)
        out.append("[[block]]\n" + body)
    open(path, "w", encoding="utf-8").write("".join(out))
    print(stats)


if __name__ == "__main__":
    main(sys.argv[1])
