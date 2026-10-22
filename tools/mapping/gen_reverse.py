#!/usr/bin/env python3
"""Generate / check the REVERSE block mapping (Vintage Story 1.22.7 -> Minecraft Java 1.21.1).

Usage (from the repo root):

    python3 tools/mapping/gen_reverse.py            # write assets/mapping_vs_to_mc.toml + check it
    python3 tools/mapping/gen_reverse.py --check    # only check the existing file

Inputs:
    assets/mapping.toml              forward mapping MC -> VS (exact/equivalent rules are inverted)
    assets/vs_block_codes.txt        every VS code (coverage target)
    assets/vs_block_absorption.tsv   light absorption per code (99 = opaque, default) -> [default]
    assets/vs_block_light.tsv        light emitted by VS blocks (creative lights)
    ~/.minecraft/versions/1.21.1/1.21.1.jar   blockstates used to validate every MC state
    ~/.local/share/vintagestory/assets        optional: creative block colours (textures)

How the file is built:
    1. FAMILY rules (this script, `family_rules()`): globs per VS family, expanded per
       material/orientation, so every orientation-carrying segment is spelled out.
    2. INVERSION of mapping.toml: every `exact`/`equivalent` rule gives a candidate
       (VS code -> MC id + `when` props). The best candidate per VS code is kept, unless
       the VS code is owned by an "authoritative" family rule (families where the reverse
       choice deliberately differs from the forward one: rocks, woods, colours...), or the
       inverted entry is identical to what the family rules already give.
    3. Output order: inverted exact codes, then family rules (exact codes first in each family,
       specific globs before broad ones). First match wins.

The check (always run) reports: coverage (exact / glob / default), dead or shadowed rules,
invalid MC ids / properties / values against the 1.21.1 jar blockstates (+ a small table of
properties that blockstate files do not list: waterlogged, leaves distance, sign rotation...).
Requires `tomli` (Python 3.10) or Python 3.11+.
"""
import argparse
import collections
import json
import math
import os
import re
import sys
import zipfile

try:
    import tomllib as toml_reader
except ImportError:  # Python < 3.11
    import tomli as toml_reader

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
ASSETS = os.path.join(ROOT, 'assets')
OUT = os.path.join(ASSETS, 'mapping_vs_to_mc.toml')
MC_JAR = os.path.expanduser('~/.minecraft/versions/1.21.1/1.21.1.jar')
VS_ASSETS = os.path.expanduser('~/.local/share/vintagestory/assets')

DIRS = ['north', 'east', 'south', 'west']
OPP = {'north': 'south', 'south': 'north', 'east': 'west', 'west': 'east', 'up': 'down', 'down': 'up'}
AXIS = {'ud': 'y', 'ns': 'z', 'we': 'x'}
CONNS = ['empty', 'n', 'e', 's', 'w', 'ne', 'ns', 'nw', 'es', 'ew', 'sw', 'nes', 'new', 'nsw', 'esw', 'nesw']


# --------------------------------------------------------------------------------------------
# Material tables
# --------------------------------------------------------------------------------------------

# VS wood -> MC wood. quality: exact when VS and MC are the "same" species (forward mapping).
WOOD = {
    'oak': ('oak', 'exact'), 'birch': ('birch', 'exact'), 'acacia': ('acacia', 'exact'),
    'pine': ('spruce', 'exact'), 'kapok': ('jungle', 'exact'), 'walnut': ('dark_oak', 'exact'),
    'ebony': ('dark_oak', 'equivalent'), 'baldcypress': ('mangrove', 'equivalent'),
    'redwood': ('mangrove', 'equivalent'), 'larch': ('spruce', 'equivalent'),
    'maple': ('cherry', 'equivalent'), 'crimsonkingmaple': ('cherry', 'equivalent'),
    'purpleheart': ('crimson', 'equivalent'), 'aged': ('spruce', 'equivalent'),
    'veryaged': ('spruce', 'equivalent'), 'rotten': ('spruce', 'equivalent'),
    'veryrotten': ('spruce', 'equivalent'), 'agedacacia': ('acacia', 'equivalent'),
    'agedebony': ('dark_oak', 'equivalent'), 'rottenebony': ('dark_oak', 'equivalent'),
    'genericwood': ('oak', 'equivalent'),
}
LEAVES = {  # VS leaves species -> MC leaves id
    'oak': 'oak_leaves', 'birch': 'birch_leaves', 'acacia': 'acacia_leaves', 'pine': 'spruce_leaves',
    'kapok': 'jungle_leaves', 'walnut': 'dark_oak_leaves', 'ebony': 'dark_oak_leaves',
    'baldcypress': 'mangrove_leaves', 'redwood': 'spruce_leaves', 'larch': 'spruce_leaves',
    'maple': 'oak_leaves', 'crimsonkingmaple': 'cherry_leaves', 'purpleheart': 'jungle_leaves',
}
SAPLING = {
    'oak': 'oak_sapling', 'birch': 'birch_sapling', 'acacia': 'acacia_sapling', 'pine': 'spruce_sapling',
    'kapok': 'jungle_sapling', 'walnut': 'dark_oak_sapling', 'ebony': 'dark_oak_sapling',
    'baldcypress': 'mangrove_propagule', 'redwood': 'spruce_sapling', 'larch': 'spruce_sapling',
    'maple': 'oak_sapling', 'crimsonkingmaple': 'cherry_sapling', 'purpleheart': 'jungle_sapling',
    'greenspirecypress': 'spruce_sapling', 'brownbamboo': 'bamboo_sapling',
    'greenbamboo': 'bamboo_sapling', 'brownbambooshoots': 'bamboo_sapling',
    'greenbambooshoots': 'bamboo_sapling',
}


def wid(vs_wood, kind):
    """MC id of a wood block kind for a VS wood species."""
    w = WOOD[vs_wood][0]
    if w == 'crimson':
        kind = {'log': 'stem', 'stripped_log': 'stripped_stem'}.get(kind, kind)
    if kind.startswith('stripped_'):
        return f'minecraft:stripped_{w}_{kind[9:]}'
    return f'minecraft:{w}_{kind}'


def wq(vs_wood):
    return WOOD[vs_wood][1]


def S(block, stairs, slab, wall, cracked=None, mossy=None, chiseled=None, props=None):
    return dict(block=block, stairs=stairs, slab=slab, wall=wall, cracked=cracked or block,
                mossy=mossy or block, chiseled=chiseled or block, props=props or {})


SETS = {
    'cobblestone': S('cobblestone', 'cobblestone_stairs', 'cobblestone_slab', 'cobblestone_wall',
                     mossy='mossy_cobblestone'),
    'stonebricks': S('stone_bricks', 'stone_brick_stairs', 'stone_brick_slab', 'stone_brick_wall',
                     cracked='cracked_stone_bricks', mossy='mossy_stone_bricks',
                     chiseled='chiseled_stone_bricks'),
    'smoothstone': S('smooth_stone', 'stone_stairs', 'smooth_stone_slab', 'cobblestone_wall'),
    'granite': S('granite', 'granite_stairs', 'granite_slab', 'granite_wall'),
    'pgranite': S('polished_granite', 'polished_granite_stairs', 'polished_granite_slab', 'granite_wall'),
    'andesite': S('andesite', 'andesite_stairs', 'andesite_slab', 'andesite_wall'),
    'pandesite': S('polished_andesite', 'polished_andesite_stairs', 'polished_andesite_slab', 'andesite_wall'),
    'diorite': S('diorite', 'diorite_stairs', 'diorite_slab', 'diorite_wall'),
    'pdiorite': S('polished_diorite', 'polished_diorite_stairs', 'polished_diorite_slab', 'diorite_wall'),
    'cdeepslate': S('cobbled_deepslate', 'cobbled_deepslate_stairs', 'cobbled_deepslate_slab',
                    'cobbled_deepslate_wall'),
    'pdeepslate': S('polished_deepslate', 'polished_deepslate_stairs', 'polished_deepslate_slab',
                    'polished_deepslate_wall'),
    'deepslatebricks': S('deepslate_bricks', 'deepslate_brick_stairs', 'deepslate_brick_slab',
                         'deepslate_brick_wall', cracked='cracked_deepslate_bricks',
                         chiseled='chiseled_deepslate'),
    'deepslatetiles': S('deepslate_tiles', 'deepslate_tile_stairs', 'deepslate_tile_slab',
                        'deepslate_tile_wall', cracked='cracked_deepslate_tiles',
                        chiseled='chiseled_deepslate'),
    'tuff': S('tuff', 'tuff_stairs', 'tuff_slab', 'tuff_wall'),
    'ptuff': S('polished_tuff', 'polished_tuff_stairs', 'polished_tuff_slab', 'polished_tuff_wall'),
    'tuffbricks': S('tuff_bricks', 'tuff_brick_stairs', 'tuff_brick_slab', 'tuff_brick_wall',
                    chiseled='chiseled_tuff_bricks'),
    'blackstone': S('blackstone', 'blackstone_stairs', 'blackstone_slab', 'blackstone_wall'),
    'pblackstone': S('polished_blackstone', 'polished_blackstone_stairs', 'polished_blackstone_slab',
                     'polished_blackstone_wall'),
    'pbasalt': S('polished_basalt', 'polished_blackstone_stairs', 'polished_blackstone_slab',
                 'polished_blackstone_wall', props={'axis': 'y'}),
    'pblackstonebricks': S('polished_blackstone_bricks', 'polished_blackstone_brick_stairs',
                           'polished_blackstone_brick_slab', 'polished_blackstone_brick_wall',
                           cracked='cracked_polished_blackstone_bricks',
                           chiseled='chiseled_polished_blackstone'),
    'sandstone': S('sandstone', 'sandstone_stairs', 'sandstone_slab', 'sandstone_wall',
                   chiseled='chiseled_sandstone'),
    'cutsandstone': S('cut_sandstone', 'sandstone_stairs', 'cut_sandstone_slab', 'sandstone_wall',
                      chiseled='chiseled_sandstone'),
    'smoothsandstone': S('smooth_sandstone', 'smooth_sandstone_stairs', 'smooth_sandstone_slab',
                         'sandstone_wall'),
    'redsandstone': S('red_sandstone', 'red_sandstone_stairs', 'red_sandstone_slab', 'red_sandstone_wall',
                      chiseled='chiseled_red_sandstone'),
    'cutredsandstone': S('cut_red_sandstone', 'red_sandstone_stairs', 'cut_red_sandstone_slab',
                         'red_sandstone_wall', chiseled='chiseled_red_sandstone'),
    'smoothredsandstone': S('smooth_red_sandstone', 'smooth_red_sandstone_stairs',
                            'smooth_red_sandstone_slab', 'red_sandstone_wall'),
    'quartzbricks': S('quartz_bricks', 'quartz_stairs', 'quartz_slab', 'diorite_wall',
                      chiseled='chiseled_quartz_block'),
    'smoothquartz': S('smooth_quartz', 'smooth_quartz_stairs', 'smooth_quartz_slab', 'diorite_wall'),
    'mudbricks': S('mud_bricks', 'mud_brick_stairs', 'mud_brick_slab', 'mud_brick_wall'),
    'bricks': S('bricks', 'brick_stairs', 'brick_slab', 'brick_wall'),
    'netherbricks': S('nether_bricks', 'nether_brick_stairs', 'nether_brick_slab', 'nether_brick_wall',
                      cracked='cracked_nether_bricks', chiseled='chiseled_nether_bricks'),
    'endstonebricks': S('end_stone_bricks', 'end_stone_brick_stairs', 'end_stone_brick_slab',
                        'end_stone_brick_wall'),
    'obsidian': S('obsidian', 'blackstone_stairs', 'blackstone_slab', 'blackstone_wall'),
}

# VS rock -> (natural MC block, natural quality, cobble set, bricks set, polished set).
# Natural blocks were chosen by average texture colour (VS rock textures vs MC textures):
# VS granite is a plain warm grey (-> stone), VS tuff is brown, VS chert is brown...
ROCK = {
    'andesite':     ('andesite', 'exact', 'cobblestone', 'stonebricks', 'pandesite'),
    'basalt':       ('smooth_basalt', 'equivalent', 'blackstone', 'pblackstonebricks', 'pbasalt'),
    'bauxite':      ('red_sandstone', 'substitute', 'redsandstone', 'cutredsandstone', 'smoothredsandstone'),
    'chalk':        ('calcite', 'equivalent', 'diorite', 'quartzbricks', 'smoothquartz'),
    'chert':        ('granite', 'substitute', 'granite', 'mudbricks', 'pgranite'),
    'claystone':    ('packed_mud', 'substitute', 'granite', 'mudbricks', 'pgranite'),
    'conglomerate': ('dripstone_block', 'substitute', 'granite', 'mudbricks', 'pgranite'),
    'granite':      ('stone', 'equivalent', 'cobblestone', 'stonebricks', 'smoothstone'),
    'greenmarble':  ('diorite', 'substitute', 'diorite', 'quartzbricks', 'pdiorite'),
    'halite':       ('white_terracotta', 'substitute', 'diorite', 'quartzbricks', 'pdiorite'),
    'kimberlite':   ('tuff', 'equivalent', 'tuff', 'tuffbricks', 'ptuff'),
    'limestone':    ('diorite', 'substitute', 'cobblestone', 'stonebricks', 'pdiorite'),
    'obsidian':     ('obsidian', 'exact', 'obsidian', 'pblackstonebricks', 'pblackstone'),
    'peridotite':   ('tuff', 'equivalent', 'tuff', 'tuffbricks', 'ptuff'),
    'phyllite':     ('andesite', 'equivalent', 'cobblestone', 'stonebricks', 'pandesite'),
    'redmarble':    ('white_terracotta', 'substitute', 'granite', 'pgranite', 'pgranite'),
    'sandstone':    ('sandstone', 'exact', 'sandstone', 'cutsandstone', 'smoothsandstone'),
    'scoria':       ('tuff', 'substitute', 'tuff', 'tuffbricks', 'ptuff'),
    'shale':        ('deepslate', 'equivalent', 'cdeepslate', 'deepslatebricks', 'pdeepslate'),
    'slate':        ('deepslate', 'equivalent', 'cdeepslate', 'deepslatetiles', 'pdeepslate'),
    'suevite':      ('tuff', 'equivalent', 'tuff', 'tuffbricks', 'ptuff'),
    'travertine':   ('white_terracotta', 'substitute', 'cutsandstone', 'cutsandstone', 'smoothsandstone'),
    'tuff':         ('tuff', 'equivalent', 'tuff', 'tuffbricks', 'ptuff'),
    'whitemarble':  ('calcite', 'equivalent', 'diorite', 'quartzbricks', 'smoothquartz'),
    'mixed':        ('stone', 'substitute', 'cobblestone', 'stonebricks', 'smoothstone'),
}
DEEPSLATE_HOSTS = {r for r, v in ROCK.items() if v[0] == 'deepslate'}
NAT_PROPS = {'deepslate': {'axis': 'y'}, 'basalt': {'axis': 'y'}}

# Fired clay colours (bricks, shingles, roofing): averaged from VS textures.
CLAY_BRICKS = {'black': 'pblackstonebricks', 'brown': 'mudbricks', 'cream': 'endstonebricks',
               'fire': 'endstonebricks', 'gray': 'stonebricks', 'orange': 'bricks', 'red': 'bricks',
               'tan': 'mudbricks', 'blue': 'deepslatebricks', 'clinker': 'netherbricks'}
CLAY_TERRACOTTA = {'black': 'gray_terracotta', 'blue': 'brown_terracotta', 'brown': 'brown_terracotta',
                   'cream': 'white_terracotta', 'earthyorange': 'brown_terracotta',
                   'fire': 'white_terracotta', 'gray': 'cyan_terracotta', 'orange': 'orange_terracotta',
                   'red': 'red_terracotta', 'tan': 'terracotta', 'travertine': 'white_terracotta'}
DAUB = {'ash': 'light_gray_concrete', 'blue': 'gray_concrete', 'brown': 'brown_terracotta',
        'browngolden': 'terracotta', 'brownlight': 'white_terracotta',
        'brownweathered': 'light_gray_terracotta', 'green': 'lime_terracotta', 'orange': 'terracotta',
        'pink': 'pink_terracotta', 'tan': 'light_gray_concrete', 'yellow': 'yellow_terracotta'}
GLASS = {'plain': 'glass', 'quartz': 'glass', 'vintage': 'glass', 'blue': 'blue_stained_glass',
         'brown': 'brown_stained_glass', 'green': 'green_stained_glass', 'pink': 'pink_stained_glass',
         'red': 'red_stained_glass', 'yellow': 'yellow_stained_glass', 'violet': 'purple_stained_glass',
         'smoky': 'gray_stained_glass'}
CARPET = {'black': 'black_carpet', 'blue': 'blue_carpet', 'brown': 'brown_carpet',
          'purple': 'purple_carpet', 'red': 'red_carpet', 'turqoise': 'cyan_carpet',
          'turqoise2': 'light_blue_carpet'}
METAL = {}
for _m in ['iron', 'steel', 'meteoriciron', 'stainlesssteel', 'nickel', 'chromium', 'titanium', 'platinum',
           'silver', 'tin', 'zinc', 'lead', 'uranium', 'cupronickel', 'bismuth']:
    METAL[_m] = ('iron_block', 'iron_block', 'iron_block', 'iron_block')
for _m in ['gold', 'electrum', 'brass']:
    METAL[_m] = ('gold_block', 'gold_block', 'gold_block', 'gold_block')
for _m in ['tinbronze', 'bismuthbronze', 'blackbronze', 'molybdochalkos']:
    METAL[_m] = ('exposed_copper', 'exposed_cut_copper', 'weathered_copper', 'weathered_cut_copper')
METAL['copper'] = ('copper_block', 'cut_copper', 'oxidized_copper', 'oxidized_cut_copper')

# Ore minerals -> MC ore id (stone host) ; None = no MC ore, keep the host rock.
ORE = {
    'bituminouscoal': 'coal_ore', 'lignite': 'coal_ore', 'anthracite': 'coal_ore', 'graphite': 'coal_ore',
    'nativecopper': 'copper_ore', 'malachite': 'copper_ore',
    'hematite': 'iron_ore', 'magnetite': 'iron_ore', 'limonite': 'iron_ore',
    'quartz_nativegold': 'gold_ore',
    'cinnabar': 'redstone_ore', 'rhodochrosite': 'redstone_ore', 'corundum': 'redstone_ore',
    'lapislazuli': 'lapis_ore', 'diamond': 'diamond_ore', 'emerald': 'emerald_ore',
    'olivine_peridot': 'emerald_ore',
    'cassiterite': 'iron_ore', 'sphalerite': 'iron_ore', 'bismuthinite': 'iron_ore',
    'pentlandite': 'iron_ore', 'chromite': 'iron_ore', 'ilmenite': 'iron_ore', 'galena': 'iron_ore',
    'galena_nativesilver': 'iron_ore', 'quartz_nativesilver': 'iron_ore', 'uranium': 'iron_ore',
}
ORE_EXACT = {'nativecopper', 'hematite', 'magnetite', 'limonite', 'quartz_nativegold', 'lapislazuli',
             'diamond', 'emerald', 'bituminouscoal', 'lignite', 'anthracite'}

FLOWER = {
    'bluebell': 'blue_orchid', 'catmint': 'allium', 'cornflower': 'cornflower', 'cowparsley': 'azure_bluet',
    'daffodil': 'dandelion', 'edelweiss': 'azure_bluet', 'forgetmenot': 'blue_orchid',
    'ghostpipepink': 'pink_tulip', 'ghostpipered': 'poppy', 'ghostpipewhite': 'lily_of_the_valley',
    'goldenpoppy': 'dandelion', 'heather': 'pink_tulip', 'horsetail': 'fern',
    'lilyofthevalley': 'lily_of_the_valley', 'mugwort': 'fern', 'orangemallow': 'orange_tulip',
    'rafflesia': 'red_tulip', 'redtopgrass': 'short_grass', 'westerngorse': 'dandelion',
    'wilddaisy': 'oxeye_daisy', 'woad': 'dandelion',
}
LUPINE = {'blue': 'cornflower', 'purple': 'allium', 'red': 'red_tulip', 'white': 'white_tulip',
          'orange': 'orange_tulip'}
RED_MUSHROOMS = {'flyagaric', 'redwinecap', 'lobster', 'sickener', 'jackolantern', 'devilbolete',
                 'chickenofthewoods', 'saffronmilkcap', 'golddropmilkcap', 'reishi', 'pinkbonnet'}
CORAL = {'blue': 'tube', 'purple': 'bubble', 'red': 'fire', 'orange': 'fire', 'yellow': 'horn',
         'pink': 'brain', 'green': 'tube', 'brown': 'dead_horn', 'tan': 'dead_brain', 'dead': 'dead_tube',
         'mix1': 'brain', 'mix2': 'bubble', 'mix3': 'fire', 'mix4': 'horn', 'stripe1': 'tube',
         'stripe2': 'brain', 'stripe3': 'horn'}
CROP_KIND = {  # VS crop -> (MC crop id, max MC age)
    'spelt': ('wheat', 7), 'rye': ('wheat', 7), 'flax': ('wheat', 7), 'rice': ('wheat', 7),
    'amaranth': ('wheat', 7), 'sunflower': ('wheat', 7), 'carrot': ('carrots', 7),
    'parsnip': ('carrots', 7), 'turnip': ('beetroots', 3), 'onion': ('beetroots', 3),
    'cabbage': ('beetroots', 3), 'cassava': ('potatoes', 7), 'peanut': ('potatoes', 7),
    'soybean': ('potatoes', 7), 'pineapple': ('potatoes', 7), 'fennel': ('carrots', 7),
    'licorice': ('carrots', 7), 'pumpkin': ('pumpkin_stem', 7),
}
CONCRETE = ['white', 'orange', 'magenta', 'light_blue', 'yellow', 'lime', 'pink', 'gray', 'light_gray',
            'cyan', 'purple', 'blue', 'brown', 'green', 'red', 'black']


# --------------------------------------------------------------------------------------------
# Rule container
# --------------------------------------------------------------------------------------------

class Rule:
    __slots__ = ('vs', 'mc', 'props', 'quality', 'note', 'auth', 'origin', 'regex')

    def __init__(self, vs, mc, props=None, quality='equivalent', note=None, auth=True, origin='family'):
        self.vs = vs if vs.startswith('game:') else 'game:' + vs
        self.mc = mc if mc.startswith('minecraft:') else 'minecraft:' + mc
        self.props = {k: str(v).lower() if isinstance(v, bool) else str(v) for k, v in (props or {}).items()}
        self.quality, self.note, self.auth, self.origin = quality, note, auth, origin
        self.regex = re.compile('^' + '.*'.join(map(re.escape, self.vs.split('*'))) + '$') if '*' in self.vs else None

    def match(self, code):
        return self.regex.match(code) is not None if self.regex else self.vs == code

    def key(self):
        return (self.mc, tuple(sorted(self.props.items())))


# --------------------------------------------------------------------------------------------
# Family rules
# --------------------------------------------------------------------------------------------

def family_rules(ctx):
    R = []

    def add(vs, mc, props=None, q='equivalent', note=None, auth=True):
        R.append(Rule(vs, mc, props, q, note, auth))

    def removed(vs, note):
        add(vs, 'air', None, 'removed', note)

    rocks = ctx['rocks']
    # ---------------- technical
    add('air', 'air', q='exact')
    add('mantle', 'bedrock', q='exact')
    removed('multiblock-*', 'VS multiblock filler; the converter must rebuild the upper half from the '
            'controller block (door upper half, tall plants...)')
    removed('meta-*', 'worldgen/structure meta block')
    for f in ['worldgenhook-*', 'vertexeater', 'slopetestobj-*', 'devplate-*', 'tobtlocator-*',
              'tickerblock-*', 'randomizer-*', 'conditionalblock-*', 'initcommandblock-*', 'decorlore-*']:
        removed(f, 'technical/creative block')
    add('commandblock-*', 'command_block', {'facing': 'north'}, 'substitute', 'command not converted')
    for n, lvl in ctx['light'].items():
        if n.startswith('game:creativelight-'):
            add(n, 'light', {'level': min(15, lvl)}, 'equivalent', 'invisible light source')
    add('creativelight-*', 'light', {'level': 15}, 'equivalent')
    for code, col in sorted(ctx['creative_colours'].items()):
        add(code, f'{col}_concrete', None, 'substitute', 'creative colour block, nearest concrete colour')
    add('creativeblock-*', 'white_concrete', None, 'substitute')
    add('creativeglow-*', 'white_concrete', None, 'substitute', 'light emission lost')
    add('creativegrass-*', 'short_grass', None, 'equivalent')

    # ---------------- liquids (VS: separate fluid layer)
    for liq, mc, q in [('water', 'water', 'exact'), ('saltwater', 'water', 'equivalent'),
                       ('rapidwater', 'water', 'equivalent'), ('boilingwater', 'water', 'equivalent'),
                       ('lava', 'lava', 'exact')]:
        for n in range(1, 8):
            add(f'{liq}-still-{n}', mc, {'level': 7 - n}, q if n == 7 else 'equivalent',
                'source' if n == 7 else 'still liquid of decreasing height')
            add(f'{liq}-d-{n}', mc, {'level': 8}, 'equivalent', 'falling liquid')
            add(f'{liq}-*-{n}', mc, {'level': max(1, 7 - n)}, 'equivalent', 'flowing liquid (direction lost)')
    add('rivulet-*', 'water', {'level': 0}, 'substitute', 'small stream block')

    # ---------------- ice / snow
    add('snowlayer-*', 'snow', None)  # replaced below by exact codes
    R.pop()
    for n in range(1, 8):
        add(f'snowlayer-{n}', 'snow', {'layers': n}, 'exact')
    add('snowblock', 'snow_block', None, 'exact')
    add('lakeice', 'ice', None, 'exact')
    add('glacierice', 'blue_ice', None, 'equivalent')
    add('packedglacierice', 'packed_ice', None, 'exact')
    add('ice-glacier', 'blue_ice', None, 'equivalent')
    add('ice-packedglacier', 'packed_ice', None, 'exact')

    # ---------------- soils
    for fert in ['verylow', 'low', 'medium', 'compost', 'high']:
        dirt = 'coarse_dirt' if fert in ('verylow', 'low') else 'dirt'
        add(f'soil-{fert}-none', dirt, None, 'exact' if fert == 'medium' else 'equivalent')
        add(f'soil-{fert}-*', 'grass_block', {'snowy': 'false'}, 'exact' if fert == 'medium' else 'equivalent',
            'normal/sparse/verysparse grass cover')
    add('forestfloor-*', 'podzol', {'snowy': 'false'}, 'equivalent', 'leaf litter covered soil')
    add('farmland-dry-*', 'farmland', {'moisture': 0}, 'equivalent', 'fertility/nutrients lost')
    add('farmland-moist-*', 'farmland', {'moisture': 7}, 'equivalent', 'fertility/nutrients lost')
    add('peat-*', 'mud', None, 'substitute')
    add('peatbrick', 'mud_bricks', None, 'substitute')
    add('peatpile', 'packed_mud', None, 'substitute')
    add('cob-*', 'packed_mud', None, 'substitute', 'cob (clay + straw) building block')
    add('rammed-*', 'packed_mud', None, 'substitute', 'rammed earth')
    add('packeddirt-trampledearth', 'dirt_path', None, 'equivalent')
    add('packeddirt', 'coarse_dirt', None, 'substitute')
    add('drypackeddirt', 'coarse_dirt', None, 'substitute')
    add('bonysoil*', 'soul_soil', None, 'substitute', 'soil with bones')
    add('devastatedsoil-*', 'coarse_dirt', None, 'substitute')
    add('rawclay-*', 'clay', None, 'equivalent', 'clay colour lost')
    add('ethoslab', 'clay', None, 'substitute')
    add('hardenedclay-*', 'terracotta', None)
    R.pop()
    for c, t in [('blue', 'brown_terracotta'), ('brown', 'brown_terracotta'), ('fire', 'white_terracotta'),
                 ('red', 'red_terracotta')]:
        add(f'hardenedclay-{c}', t, None, 'equivalent')
    add('termitemound-*', 'coarse_dirt', None, 'substitute')
    add('dirtygravel-*', 'gravel', None, 'substitute')
    add('muddygravel', 'gravel', None, 'substitute')
    add('sludgygravel', 'gravel', None, 'substitute')
    removed('transition-*', 'ground transition decal')
    add('rocktyped-rubble-*', 'gravel', None, 'substitute')
    # sand / gravel (layered: 1..7 = 2..14 voxels)
    for r in rocks:
        sand = 'red_sand' if r == 'bauxite' else 'sand'
        for fam, mc in [('gravel', 'gravel'), ('sand', sand)]:
            if f'game:{fam}-{r}' in ctx['codes']:
                add(f'{fam}-{r}', mc, None, 'exact' if (fam, r) in (('sand', 'sandstone'),) else 'equivalent')
    for n in range(1, 8):
        for r in rocks:
            sand = 'red_sand' if r == 'bauxite' else 'sand'
            if n >= 4:
                add(f'sand-{r}-{n}', sand, None, 'equivalent', 'layered sand (half block or more) -> full block')
        if n >= 4:
            add(f'gravel-*-{n}', 'gravel', None, 'equivalent', 'layered gravel (half block or more) -> full block')
    add('sand-*', 'air', None, 'removed', 'thin sand layer (less than half a block)')
    add('gravel-*', 'air', None, 'removed', 'thin gravel layer (less than half a block)')
    for r in rocks:
        add(f'sandwavy-{r}', 'red_sand' if r == 'bauxite' else 'sand', None, 'equivalent')

    # ---------------- rocks
    for r, (nat, q, cob, bri, pol) in ROCK.items():
        nprops = NAT_PROPS.get(nat)
        note = None if q == 'exact' else f'VS {r} (colour match)'
        for fam in ['rock', 'crackedrock']:
            if f'game:{fam}-{r}' in ctx['codes']:
                add(f'{fam}-{r}', nat, nprops, q if fam == 'rock' else 'equivalent', note)
        cs, bs, ps = SETS[cob], SETS[bri], SETS[pol]
        for fam, mc, pr in [('cobblestone', cs['block'], cs['props']), ('drystone', cs['block'], cs['props']),
                            ('cobbleskull', cs['block'], cs['props']),
                            ('mossycobblestone', cs['mossy'], cs['props']),
                            ('lichencobblestone', cs['block'], cs['props']),
                            ('stonebricks', bs['block'], bs['props']),
                            ('crackedstonebricks', bs['cracked'], bs['props']),
                            ('agedstonebricks', bs['cracked'], bs['props']),
                            ('mossystonebricks', bs['mossy'], bs['props']),
                            ('lichenstonebricks', bs['block'], bs['props']),
                            ('rockpolished', ps['block'], ps['props']),
                            ('mossyrockpolished', ps['block'], ps['props']),
                            ('lichenrockpolished', ps['block'], ps['props']),
                            ('polishedrockold-full', ps['block'], ps['props']),
                            ('polishedrockold-cobbled', cs['block'], cs['props'])]:
            if f'game:{fam}-{r}' in ctx['codes']:
                add(f'{fam}-{r}', mc, pr)
        if any(c.startswith(f'game:cobblestonefan-{r}-') for c in ctx['codes']):
            add(f'cobblestonefan-{r}-*', cs['block'], cs['props'], 'equivalent', 'fanned cobblestone pattern')
        for fam, st in [('cobblestonestairs', cs), ('stonebrickstairs', bs)]:
            stairs(add, f'{fam}-{r}', st['stairs'])
        for fam, st in [('cobblestoneslab', cs), ('stonebrickslab', bs), ('polishedrockslab', ps)]:
            slabs(add, f'{fam}-{r}', st['slab'], st['block'], st['props'])
        walls(add, f'drystonefence-{r}', cs['wall'])
        if any(c.startswith(f'game:stalagsection-{r}-') for c in ctx['codes']):
            for size, th in [('04', 'tip'), ('06', 'frustum'), ('08', 'middle'), ('10', 'middle'),
                             ('12', 'base'), ('14', 'base')]:
                add(f'stalagsection-{r}-{size}', 'pointed_dripstone',
                    {'thickness': th, 'vertical_direction': 'up'}, 'substitute',
                    'stalagmite/stalactite section; direction unknown (up)')
    add('cobblestone-coral', 'dead_brain_coral_block', None, 'substitute')
    add('stonecoffinsection-*', 'polished_andesite_slab', {'type': 'bottom'}, 'substitute')
    add('stonecoffinlid-*', 'polished_andesite_slab', {'type': 'bottom'}, 'substitute')
    add('diamond-stone-*', 'chiseled_stone_bricks', None, 'substitute', 'diamond-pattern stone')
    add('chiseledblock*', 'stone', None, 'substitute',
        'chiseled block: shape and materials live in the block entity (could use its main material)')
    add('microblock*', 'stone', None, 'substitute',
        'microblock: shape and materials live in the block entity (could use its main material)')
    add('altar-*', 'chiseled_stone_bricks', None, 'substitute')
    add('quern-*', 'grindstone', {'face': 'floor', 'facing': 'north'}, 'substitute')
    add('meteorite-iron', 'raw_iron_block', None, 'substitute')
    add('drock', 'blackstone', None, 'substitute', 'devastation rock')
    add('stonepathfullblock', 'smooth_stone', None, 'substitute')
    add('stonepathslab-*', 'smooth_stone_slab', {'type': 'bottom'}, 'substitute')
    stairs(add, 'stonepathstairs', 'stone_stairs')
    add('stonepath-*', 'smooth_stone_slab', {'type': 'bottom'}, 'substitute', 'stone path (low block)')

    # ---------------- ores
    for m, ore in ORE.items():
        q = 'equivalent' if m in ORE_EXACT else 'substitute'
        for r in sorted(DEEPSLATE_HOSTS):
            add(f'ore-*{m}-{r}', 'deepslate_' + ore, None, q, 'deepslate host rock')
        add(f'ore-*-{m}-*', ore, None, q, 'grade lost')
        add(f'ore-{m}-*', ore, None, q)
    for r in rocks:
        nat = ROCK.get(r, ('stone',))[0] if r in ROCK else ('white_terracotta' if r == 'halite' else 'stone')
        add(f'ore-*-{r}', nat, NAT_PROPS.get(nat), 'substitute',
            'mineral without MC ore (alum, borax, sulfur, quartz...): host rock kept')
    for r in ['halite', 'travertine']:
        add(f'ore-*-{r}', ROCK[r][0], None, 'substitute', 'mineral without MC ore: host rock kept')
    for f in ['looseores-*', 'loosestones-*', 'looseflints-*', 'looseboulders-*', 'loosestick-*',
              'loosegears-*', 'stickslayer-*', 'seashell-*', 'flotsam-*']:
        removed(f, 'small loose item on the ground')
    for mineral in ['amethyst', 'milkyquartz', 'olivine', 'rosequartz', 'smokyquartz']:
        q = 'exact' if mineral == 'amethyst' else 'substitute'
        for d in ['up', 'down'] + DIRS:
            add(f'crystal-{mineral}-small*-{d}', 'small_amethyst_bud', {'facing': d}, q)
            add(f'crystal-{mineral}-large*-{d}', 'large_amethyst_bud', {'facing': d}, q)
            add(f'crystal-{mineral}-cluster*-{d}', 'amethyst_cluster', {'facing': d}, q)
    add('crystallayer-*', 'small_amethyst_bud', {'facing': 'up'}, 'substitute')
    add('saltpeter-*', 'air', None, 'removed', 'saltpeter crust')

    # ---------------- wood
    for w in WOOD:
        for st in ['grown', 'placed', 'resin', 'resinharvested']:
            for a, ax in AXIS.items():
                if f'game:log-{st}-{w}-{a}' in ctx['codes']:
                    add(f'log-{st}-{w}-{a}', wid(w, 'log'), {'axis': ax}, wq(w))
        for a, ax in AXIS.items():
            if f'game:debarkedlog-{w}-{a}' in ctx['codes']:
                add(f'debarkedlog-{w}-{a}', wid(w, 'stripped_log'), {'axis': ax}, wq(w))
            if f'game:planks-{w}-{a}' in ctx['codes']:
                add(f'planks-{w}-{a}', wid(w, 'planks'), None, wq(w))
        for kind, mc in [('placed', 'log'), ('barkedcorner', 'log'), ('debarked', 'stripped_log'),
                         ('debarkedcorner', 'stripped_log')]:
            for a, ax in list(AXIS.items()) + [(d, 'y') for d in DIRS]:
                if f'game:logquad-{kind}-{w}-{a}' in ctx['codes']:
                    add(f'logquad-{kind}-{w}-{a}', wid(w, mc), {'axis': ax}, wq(w), 'quarter of a giant log')
        if w in ('rotten', 'veryrotten', 'genericwood', 'agedacacia', 'agedebony', 'rottenebony'):
            continue
        stairs(add, f'plankstairs-{w}', wid(w, 'stairs'), wq(w))
        slabs(add, f'plankslab-{w}', wid(w, 'slab'), wid(w, 'planks'), None, wq(w))
        fences(add, f'woodenfence-{w}', wid(w, 'fence'), wq(w))
        fences(add, f'roughhewnfence-{w}', wid(w, 'fence'), 'equivalent')
        gates(add, f'woodenfencegate-{w}', wid(w, 'fence_gate'), wq(w))
        gates(add, f'roughhewnfencegate-{w}', wid(w, 'fence_gate'), 'equivalent')
        add(f'door-solid-{w}', wid(w, 'door'), {'half': 'lower'}, wq(w),
            'orientation/hinge/open state live in the block entity (BEBehaviorDoor)')
        add(f'door-sleek-windowed-{w}', wid(w, 'door'), {'half': 'lower'}, 'equivalent',
            'orientation/hinge/open state live in the block entity')
        add(f'door-*-{w}', wid(w, 'door'), {'half': 'lower'}, 'equivalent',
            'multi-block VS door/gate; orientation in the block entity')
        add(f'trapdoor-solid-{w}-*', wid(w, 'trapdoor'), {'half': 'bottom'}, wq(w),
            'orientation/open state live in the block entity')
        add(f'trapdoor-window-{w}-*', wid(w, 'trapdoor'), {'half': 'bottom'}, 'equivalent',
            'orientation/open state live in the block entity')
        for d in DIRS:
            add(f'ladder-wood-{w}-{d}', 'ladder', {'facing': OPP[d]}, 'exact' if w == 'oak' else 'equivalent',
                'VS ladder side = wall it hangs on, opposite of MC facing')
        for a, mcprops in [('ns', {'east': 'true', 'west': 'true'}), ('ew', {'north': 'true', 'south': 'true'})]:
            add(f'glasspane-leaded-{w}-{a}', 'glass_pane', mcprops, 'equivalent',
                'VS -ns spans east-west, -ew spans north-south')
        if f'game:woodenpath-{w}-ns' in ctx['codes']:
            add(f'woodenpath-{w}-*', wid(w, 'slab'), {'type': 'bottom'}, 'substitute', 'boardwalk')
        add(f'wildbeehive-inlog-{w}', 'bee_nest', {'facing': 'north'}, 'substitute')
    add('genericwood', 'oak_planks', None, 'equivalent')
    for w, mc in LEAVES.items():
        for fam in ['leaves', 'leavesbranchy']:
            add(f'{fam}-placed-{w}', mc, {'persistent': 'true', 'distance': 1},
                'exact' if w in ('oak', 'birch', 'acacia', 'pine', 'kapok', 'walnut') else 'equivalent')
            add(f'{fam}-grown*-{w}', mc, {'persistent': 'false', 'distance': 1},
                'exact' if w in ('oak', 'birch', 'acacia', 'pine', 'kapok', 'walnut') else 'equivalent',
                'distance=1 so MC does not decay the tree before the first update')
    add('leavesbranchystatic-*', 'birch_leaves', {'persistent': 'true', 'distance': 1}, 'equivalent')
    add('leavesnarrow-placed-*', 'spruce_leaves', {'persistent': 'true', 'distance': 1}, 'equivalent', 'cypress')
    add('leavesnarrow-*', 'spruce_leaves', {'persistent': 'false', 'distance': 1}, 'equivalent', 'cypress')
    for a, ax in AXIS.items():
        add(f'lognarrow-*-{a}', 'spruce_log', {'axis': ax}, 'substitute', 'narrow cypress log')
        add(f'logsection-*-{a}', 'mangrove_log', {'axis': ax}, 'equivalent', 'giant redwood log section')
        add(f'carvedlog-*-{a}', 'stripped_acacia_log', {'axis': ax}, 'substitute', 'carved log (totem)')
        add(f'burnedplanks-*-{a}', 'dark_oak_planks', None, 'substitute', 'burned planks')
        add(f'hay-*-{a}', 'hay_block', {'axis': ax}, 'exact')
        add(f'stackedbamboo-{a}', 'bamboo_block', {'axis': ax}, 'equivalent')
        add(f'quartzpillar-{a}', 'quartz_pillar', {'axis': ax}, 'exact')
        add(f'plaster-*-{a}', 'white_concrete', None, 'substitute', 'patterned plaster')
    add('rottenlog-stump', 'stripped_spruce_log', {'axis': 'y'}, 'substitute')
    add('rottenlog-*', 'stripped_spruce_log', {'axis': 'x'}, 'substitute', 'fallen rotten log, axis unknown')
    add('agedwallpaperplanks-*', 'spruce_planks', None, 'substitute', 'wallpaper lost')
    removed('wallpaper-*', 'wallpaper decal')
    for s, mc in SAPLING.items():
        props = {'age': 0, 'hanging': 'false'} if mc == 'minecraft:mangrove_propagule' or mc == 'mangrove_propagule' else None
        add(f'sapling-{s}-*', mc, props, 'exact' if s in ('oak', 'birch', 'acacia', 'pine', 'kapok', 'walnut') else 'equivalent')
    # bamboo
    for seg, leaves in [('segment1', 'none'), ('segment2', 'small'), ('segment3', 'large')]:
        add(f'bamboo-*-{seg}', 'bamboo', {'age': 0, 'leaves': leaves}, 'equivalent')
    add('bambooleaves-*-placed', 'jungle_leaves', {'persistent': 'true', 'distance': 1}, 'substitute')
    add('bambooleaves-*', 'jungle_leaves', {'persistent': 'false', 'distance': 1}, 'substitute')
    fences(add, 'fencebamboo-brown', 'bamboo_fence', 'equivalent')
    gates(add, 'bamboogate-brown', 'bamboo_fence_gate', 'equivalent', hinge=False)
    fences(add, 'wattle-sticks', 'spruce_fence', 'substitute')
    gates(add, 'wattlegate-sticks', 'spruce_fence_gate', 'substitute')
    add('palisadewall-*', 'spruce_fence', None, 'substitute', 'palisade (connections recomputed by MC)')
    add('palisadestakes-*', 'spruce_fence', None, 'substitute')

    # ---------------- doors / gates (code-oriented legacy ones)
    for kind, mc in [('plank', 'oak_door'), ('log', 'spruce_door')]:
        for d in DIRS:
            for half, mh in [('down', 'lower'), ('up', 'upper')]:
                for st, op in [('closed', 'false'), ('opened', 'true')]:
                    for h in ['left', 'right']:
                        add(f'door-{kind}-{d}-{half}-{st}-{h}', mc,
                            {'facing': d, 'half': mh, 'open': op, 'hinge': h}, 'equivalent',
                            'legacy door, orientation in code (same convention as irondoor)')
    for d in DIRS:
        for half, mh in [('down', 'lower'), ('up', 'upper')]:
            for st, op in [('closed', 'false'), ('opened', 'true')]:
                for h in ['left', 'right']:
                    add(f'irondoor-{d}-{half}-{st}-{h}', 'iron_door',
                        {'facing': d, 'half': mh, 'open': op, 'hinge': h}, 'exact')
    add('door-*-towergate', 'spruce_door', {'half': 'lower'}, 'substitute', 'big VS gate')
    add('door-*-barred*', 'iron_door', {'half': 'lower'}, 'substitute')
    add('door-*', 'oak_door', {'half': 'lower'}, 'substitute', 'VS door style without MC equivalent')
    add('metaldoor-*', 'iron_door', {'half': 'lower'}, 'equivalent', 'orientation in the block entity')
    add('trapdoor-bars-*', 'iron_trapdoor', {'half': 'bottom'}, 'equivalent', 'orientation in the block entity')
    add('trapdoor-plate-*', 'iron_trapdoor', {'half': 'bottom'}, 'equivalent', 'orientation in the block entity')
    for st, op in [('closed', 'false'), ('opened', 'true')]:
        for pos, half in [('up', 'top'), ('down', 'bottom'), ('left', 'bottom'), ('right', 'bottom')]:
            for d in DIRS:
                add(f'trapdoor-{st}-{pos}-{d}', 'oak_trapdoor', {'facing': d, 'open': op, 'half': half},
                    'equivalent', 'legacy trapdoor, orientation unverified')
    add('ladder-rope-*', 'ladder', None, 'substitute')
    add('ladder-stick-*', 'ladder', None, 'substitute')
    for d in DIRS:
        add(f'ladder-rope-{d}', 'ladder', {'facing': OPP[d]}, 'substitute')
        add(f'ladder-stick-{d}', 'ladder', {'facing': OPP[d]}, 'substitute')
    R[:] = [r for r in R if r.vs not in ('game:ladder-rope-*', 'game:ladder-stick-*')]

    # ---------------- glass
    for c, mc in GLASS.items():
        q = 'exact' if c in ('plain', 'blue', 'brown', 'green', 'pink', 'red', 'yellow') else 'equivalent'
        add(f'glass-{c}', mc, None, q)
        add(f'glassslab-{c}-*', mc, None, 'substitute', 'MC has no glass slab: full glass block')
    add('displaycase-*', 'glass', None, 'substitute')
    add('talldisplaycase-*', 'glass', None, 'substitute')

    # ---------------- quartz
    add('quartz-plain', 'quartz_block', None, 'exact')
    add('quartz-ornate', 'chiseled_quartz_block', None, 'exact')
    slabs(add, 'quartzslab', 'quartz_slab', 'quartz_block', None, 'exact')
    stairs(add, 'quartzstairs', 'quartz_stairs', 'exact')

    # ---------------- clay bricks, shingles, tiles, roofing
    for c, setname in CLAY_BRICKS.items():
        st = SETS[setname]
        q = 'exact' if c == 'red' else 'equivalent'
        add(f'claybricks-*-{c}', st['block'], None, q)
        add(f'brickcourse-*-{c}', st['block'], None, 'equivalent', 'decorative brick course')
        add(f'brickruin-*-{c}', st['cracked'], None, 'equivalent')
        stairs(add, f'brickstairs-*-{c}', st['stairs'], q)
        slabs(add, f'brickslabs-*-{c}', st['slab'], st['block'], None, q)
        if c in ('black', 'brown', 'cream', 'fire', 'gray', 'orange', 'red', 'tan'):
            add(f'clayshingleblock-{c}', st['block'], None, 'substitute', 'clay shingles')
            stairs(add, f'clayshinglestairs-{c}', st['stairs'], 'substitute')
            slabs(add, f'clayshinglelabs-{c}', st['slab'], st['block'], None, 'substitute')
    add('claybricks-clinkerrough', 'nether_bricks', None, 'substitute')
    st = SETS['endstonebricks']
    stairs(add, 'brickstairs-fire', st['stairs'], 'equivalent')
    slabs(add, 'brickslabs-fire', st['slab'], st['block'], None, 'equivalent')
    add('claybricks-*', 'end_stone_bricks', None, 'equivalent', 'fire bricks')
    add('claybrickchimney-*', 'bricks', None, 'substitute')
    add('refractorybricks-*', 'end_stone_bricks', None, 'substitute', 'refractory bricks')
    add('refractorybrickgrating-*', 'end_stone_bricks', None, 'substitute')
    add('bloomerychimney', 'bricks', None, 'substitute')
    add('shingleblock-copper', 'cut_copper', None, 'substitute')
    for c, t in CLAY_TERRACOTTA.items():
        add(f'tile-*-{c}', t, None, 'equivalent', 'ceramic tiles')
        add(f'tileslab-*-{c}-*', t, None, 'substitute', 'MC terracotta has no slab: full block')
    for c, t in DAUB.items():
        add(f'daub-{c}-*', t, None, 'equivalent', 'daub (colour match)')
    add('plaster-*', 'white_concrete', None, 'equivalent', 'plaster')
    add('mudbrick-light', 'mud_bricks', None, 'exact')
    add('mudbrick-dark', 'packed_mud', None, 'equivalent')
    slabs(add, 'mudbrickslab-*', 'mud_brick_slab', 'mud_bricks', None, 'exact')

    roof = {}
    for w in ['acacia', 'aged', 'baldcypress', 'birch', 'ebony', 'kapok', 'larch', 'maple', 'oak', 'pine',
              'purpleheart', 'redwood', 'walnut']:
        roof[w] = (wid(w, 'stairs'), wid(w, 'slab'), wid(w, 'planks'))
    for c in ['black', 'brown', 'cream', 'fire', 'gray', 'orange', 'red', 'tan']:
        st = SETS[CLAY_BRICKS[c]]
        roof[c + 'clay'] = (st['stairs'], st['slab'], st['block'])
    roof['slate'] = ('deepslate_tile_stairs', 'deepslate_tile_slab', 'deepslate_tiles')
    roof['copper'] = ('cut_copper_stairs', 'cut_copper_slab', 'cut_copper')
    roof['bamboo'] = ('bamboo_mosaic_stairs', 'bamboo_mosaic_slab', 'bamboo_mosaic')
    roof['thatch'] = ('hay_block', 'hay_block', 'hay_block')
    roof['agedthatch'] = ('hay_block', 'hay_block', 'hay_block')
    roof['sod'] = ('moss_block', 'moss_block', 'moss_block')
    for mat, (stid, slid, blid) in roof.items():
        if stid == 'hay_block' or stid == 'moss_block':
            add(f'slantedroofing*-{mat}-*', stid, {'axis': 'y'} if stid == 'hay_block' else None, 'substitute',
                'thatch/sod roof: no stairs in MC, full block')
            continue
        for d in DIRS:
            for piece, shape in [('slantedroofing', 'straight'), ('slantedroofingbottom', 'straight'),
                                 ('slantedroofingtop', 'straight'), ('slantedroofinghalfleft', 'straight'),
                                 ('slantedroofinghalfright', 'straight'),
                                 ('slantedroofingcornerinner', 'inner_left'),
                                 ('slantedroofingcornerouter', 'outer_left')]:
                add(f'{piece}-{mat}-{d}-*', stid, {'facing': d, 'half': 'bottom', 'shape': shape}, 'substitute',
                    'roof piece -> stairs, orientation unverified')
        add(f'slantedroofingridge*-{mat}-*', slid, {'type': 'bottom'}, 'substitute', 'roof ridge -> slab')
        add(f'slantedroofingtip-{mat}-*', slid, {'type': 'bottom'}, 'substitute', 'roof tip -> slab')
    removed('beam-*', 'roof beam: geometry stored in the block entity (BlockSupportBeam)')
    removed('supportbeam-*', 'support beam: geometry stored in the block entity')

    # ---------------- metal
    add('metalblock-corroded-*-rusty-iron', 'raw_iron_block', None, 'substitute')
    for m, (plain, riveted, cplain, criveted) in METAL.items():
        q = 'exact' if m in ('iron', 'gold', 'copper') else 'substitute'
        add(f'metalblock-new-riveted-{m}', riveted, None, q)
        add(f'metalblock-new-*-{m}', plain, None, q)
        add(f'metalblock-corroded-riveted-{m}', criveted, None, q)
        add(f'metalblock-corroded-*-{m}', cplain, None, q)
        plate = 'light_weighted_pressure_plate' if plain == 'gold_block' else 'heavy_weighted_pressure_plate'
        add(f'metalsheet-{m}-down', plate, None, 'substitute', 'metal sheet on the floor')
    removed('metalsheet-*', 'metal sheet on a wall/ceiling')
    fences_iron(add)
    add('supportchain-*', 'chain', {'axis': 'y'}, 'equivalent')
    add('supportrope-*', 'chain', {'axis': 'y'}, 'substitute', 'rope')
    add('anvil-*', 'anvil', {'facing': 'north'}, 'equivalent')
    add('anvilpart-*', 'damaged_anvil', {'facing': 'north'}, 'substitute')
    add('lightningrod', 'lightning_rod', {'facing': 'up'}, 'exact')

    # ---------------- cloth / furniture
    for c in ['black', 'brown', 'white']:
        add(f'wool-{c}-*', f'{c}_wool', None, 'exact')
    add('linen-*', 'white_wool', None, 'substitute')
    add('leather', 'brown_wool', None, 'substitute')
    for c, mc in CARPET.items():
        add(f'smallcarpet-{c}', mc, None, 'equivalent')
        add(f'mediumcarpet-{c}-*', mc, None, 'equivalent')
    add('rug-*', 'blue_carpet', None, 'equivalent')
    add('rushmat', 'yellow_carpet', None, 'substitute')
    add('strawbedding', 'yellow_carpet', None, 'substitute')
    for bt, col in [('wood', 'red'), ('woodaged', 'brown'), ('hay', 'yellow'), ('village', 'white')]:
        for part, mp in [('feet', 'foot'), ('head', 'head')]:
            for d in DIRS:
                add(f'bed-{bt}-{part}-{d}', f'{col}_bed', {'part': mp, 'facing': OPP[d]}, 'equivalent',
                    'VS bed side = opposite of MC facing')
    for d in DIRS:
        add(f'sign-wall-{d}', 'oak_wall_sign', {'facing': OPP[d]}, 'equivalent',
            'VS sign-wall side = edge of the supporting block, opposite of MC facing; text in BE')
        add(f'sign-ground-{d}', 'oak_sign', {'rotation': {'south': 0, 'west': 4, 'north': 8, 'east': 12}[d]},
            'equivalent', 'rotation is a guess; the real angle and text are in the block entity')
        add(f'chest-{d}', 'chest', {'facing': d}, 'exact', 'contents in block entity')
        add(f'labeledchest-{d}', 'chest', {'facing': d}, 'equivalent', 'contents/label in block entity')
        add(f'trunk-{d}', 'chest', {'facing': d}, 'substitute', 'VS trunk is 2 blocks wide')
        add(f'clayoven-{d}', 'furnace', {'facing': d}, 'substitute', 'orientation unverified')
        add(f'bloomerybase-{d}', 'blast_furnace', {'facing': d}, 'substitute')
        for lit in ['lit', 'unlit']:
            add(f'stove-{lit}-{d}', 'smoker', {'facing': d, 'lit': str(lit == 'lit').lower()}, 'substitute')
        add(f'resonator-{d}', 'jukebox', None, 'substitute')
        add(f'skep-*-{d}', 'beehive', {'facing': d}, 'equivalent')
        add(f'torch-basic-*-{d}', 'wall_torch', {'facing': d}, 'exact')
        add(f'torchholder-*-filled-{d}', 'wall_torch', {'facing': d}, 'substitute')
        for st in ['normal', 'broken', 'unrepairable']:
            add(f'statictranslocator-{st}-{d}', 'lodestone', None, 'substitute')
    add('signpost', 'oak_fence', None, 'substitute')
    add('torch-basic-*-up', 'torch', None, 'exact', 'extinct/burned-out torches become lit torches')
    removed('torchholder-*', 'empty torch holder')
    add('lantern-*-down', 'lantern', {'hanging': 'true'}, 'equivalent', 'lantern material in block entity')
    add('lantern-*', 'lantern', {'hanging': 'false'}, 'equivalent', 'lantern material in block entity')
    add('oillamp-*-down', 'lantern', {'hanging': 'true'}, 'substitute')
    add('oillamp-*', 'lantern', {'hanging': 'false'}, 'substitute')
    add('chandelier-*', 'lantern', {'hanging': 'true'}, 'substitute')
    add('paperlantern-*', 'lantern', {'hanging': 'true'}, 'substitute')
    add('candle', 'candle', {'candles': 1, 'lit': 'false'}, 'equivalent')
    for n in range(1, 10):
        add(f'bunchocandles-{n}', 'candle', {'candles': min(4, n), 'lit': 'true'}, 'equivalent')
    add('chest-*', 'chest', None, 'exact')
    add('barrel', 'barrel', {'facing': 'up'}, 'equivalent', 'contents in block entity')
    add('stationarybasket-*', 'barrel', {'facing': 'up'}, 'substitute')
    add('crate', 'barrel', {'facing': 'up'}, 'substitute')
    add('woodencrate-*', 'barrel', {'facing': 'up'}, 'substitute')
    add('cabinet-*', 'barrel', {'facing': 'north'}, 'substitute')
    for f in ['storagevessel-*', 'lootvessel-*', 'claypot-*', 'crock-*', 'dirtyclaypot-*']:
        add(f, 'decorated_pot', None, 'substitute', 'ceramic vessel')
    add('flowerpot-*', 'flower_pot', None, 'equivalent', 'plant in the block entity')
    add('clayplanter-*', 'flower_pot', None, 'substitute')
    for f in ['bookshelf', 'bookshelves-*', 'clutteredbookshelf*', 'scrollrack']:
        add(f, 'bookshelf', None, 'equivalent')
    add('table-*', 'crafting_table', None, 'substitute', 'decorative table')
    add('chair-*', 'oak_stairs', {'facing': 'north', 'half': 'bottom'}, 'substitute', 'chair (orientation in BE)')
    add('henbox*', 'composter', {'level': 1}, 'substitute')
    add('trough-*', 'cauldron', None, 'substitute')
    add('firepit-lit', 'campfire', {'lit': 'true'}, 'equivalent')
    add('firepit-*', 'campfire', {'lit': 'false'}, 'equivalent')
    add('fire', 'fire', None, 'exact')
    add('forge', 'blast_furnace', None, 'substitute')
    add('pitkiln', 'campfire', {'lit': 'false'}, 'substitute')
    add('charcoalpit', 'coal_block', None, 'substitute')
    add('charcoalpile-*', 'coal_block', None, 'substitute')
    add('coalpile', 'coal_block', None, 'substitute')
    add('firewoodpile', 'oak_log', {'axis': 'x'}, 'substitute')
    add('plankpile', 'oak_planks', None, 'substitute')
    add('hopper', 'hopper', {'facing': 'down'}, 'exact')
    add('chute-*', 'hopper', {'facing': 'down'}, 'substitute', 'VS chute')
    for vs, shape in [('flat_ns', 'north_south'), ('flat_we', 'east_west'), ('raised_ns', 'ascending_north'),
                      ('raised_we', 'ascending_east'), ('curved_es', 'south_east'), ('curved_ne', 'north_east'),
                      ('curved_sw', 'south_west'), ('curved_wn', 'north_west')]:
        add(f'woodenrails-{vs}', 'rail', {'shape': shape}, 'equivalent', 'ascending direction unverified')
    add('bomb-*', 'tnt', None, 'substitute')
    add('pie-*', 'cake', {'bites': 0}, 'substitute')
    for f in ['teleporterbase', 'basereturnteleporter*', 'corpsereturnteleporter-*', 'riftward']:
        add(f, 'lodestone', None, 'substitute')
    add('locustnest-cage', 'spawner', None, 'substitute')
    add('wildbeehive-*', 'bee_nest', {'facing': 'north'}, 'equivalent')
    add('spiderweb', 'cobweb', None, 'exact')

    # ---------------- plants
    for h in ['eaten', 'veryshort', 'short', 'mediumshort', 'medium', 'tall', 'verytall']:
        add(f'tallgrass-{h}-*', 'short_grass', None, 'equivalent' if h in ('tall', 'verytall') else 'exact',
            'single block in VS (tall_grass would need an upper half)' if h in ('tall', 'verytall') else None)
    add('tallfern', 'fern', None, 'equivalent', 'single block in VS')
    add('fern-*', 'fern', None, 'equivalent')
    add('herb-*', 'fern', None, 'substitute', 'herb')
    for f, mc in FLOWER.items():
        add(f'flower-{f}-*', mc, None, 'exact' if f in ('cornflower', 'lilyofthevalley') else 'equivalent')
    for c, mc in LUPINE.items():
        add(f'flower-lupine-{c}-*', mc, None, 'equivalent')
    add('flower-croton-*', 'fern', None, 'substitute')
    for kind, (mc, maxage) in CROP_KIND.items():
        maxst = ctx['crop_max'].get(kind)
        if not maxst:
            continue
        for st in range(1, maxst + 1):
            age = round((st - 1) * maxage / max(1, maxst - 1))
            add(f'crop-{kind}-{st}', mc, {'age': age}, 'equivalent' if kind not in ('carrot',) else 'exact')
    add('deadcrop', 'dead_bush', None, 'substitute')
    removed('cropprop', 'crop support prop')
    for st, age in [('empty', 1), ('flowering', 2), ('ripe', 3)]:
        add(f'bigberrybush-*-{st}', 'sweet_berry_bush', {'age': age}, 'substitute')
        add(f'smallberrybush-*-{st}', 'sweet_berry_bush', {'age': age}, 'equivalent')
    add('fruitingbush-*', 'sweet_berry_bush', {'age': 2}, 'substitute')
    add('fruitingbushcutting-*', 'sweet_berry_bush', {'age': 0}, 'substitute')
    add('fruittree-foliage', 'flowering_azalea_leaves', {'persistent': 'true', 'distance': 1}, 'substitute')
    add('fruittree-cutting', 'oak_sapling', None, 'substitute')
    add('fruittree-*', 'oak_log', {'axis': 'y'}, 'substitute', 'fruit tree branch/stem')
    add('ferntree-*-foliage', 'jungle_leaves', {'persistent': 'true', 'distance': 1}, 'substitute')
    add('ferntree-*', 'jungle_log', {'axis': 'y'}, 'substitute', 'tree fern trunk')
    add('saguarocactus-*', 'cactus', None, 'equivalent')
    add('barrelcactus-*', 'cactus', None, 'substitute')
    add('silvertorchcactus', 'cactus', None, 'substitute')
    add('pumpkin-fruit-*', 'pumpkin', None, 'exact')
    removed('pumpkin-vine-*', 'pumpkin vine on the ground (no MC equivalent)')
    add('pineapple', 'fern', None, 'substitute')
    for m in ctx['mushrooms']:
        mc = 'red_mushroom' if m in RED_MUSHROOMS else 'brown_mushroom'
        add(f'mushroom-{m}-*-snow', mc, None, 'equivalent')
        for d in DIRS:
            removed(f'mushroom-{m}-*-{d}', 'shelf fungus on a tree side')
        add(f'mushroom-{m}-*', mc, None, 'equivalent')
    add('waterlily', 'lily_pad', None, 'exact')
    add('aquatic-kelp-top', 'kelp', {'age': 25}, 'exact')
    add('aquatic-kelp-*', 'kelp_plant', None, 'exact')
    add('aquatic-lilygiant-*', 'lily_pad', None, 'substitute')
    add('aquatic-lilyflower-*', 'lily_pad', None, 'substitute')
    removed('aquatic-decor-*', 'underwater decoration (barnacles, crusts...)')
    add('aquatic-*', 'seagrass', None, 'equivalent', 'underwater plant')
    add('aquaticplant-*', 'seagrass', None, 'equivalent')
    add('seaweed-top', 'kelp', {'age': 25}, 'equivalent')
    add('seaweed-section', 'kelp_plant', None, 'equivalent')
    add('coral-anemone-*', 'sea_pickle', {'pickles': 1, 'waterlogged': 'true'}, 'substitute')
    for c, sp in CORAL.items():
        add(f'coral-fan-{c}-*', f'{sp}_coral_fan', None, 'equivalent')
        add(f'coral-*-{c}-*', f'{sp}_coral', None, 'equivalent')
    for c, sp in [('blue', 'tube'), ('brown', 'dead_horn'), ('green', 'tube'), ('red', 'fire'), ('tan', 'dead_brain'),
                  ('yellow', 'horn')]:
        add(f'reefblock-{c}', f'{sp}_coral_block', None, 'equivalent')
        add(f'reefstructure-{c}', f'{sp}_coral_block', None, 'substitute')
        add(f'reefledge-{c}-*', f'{sp}_coral_block', None, 'substitute')
    walls(add, 'fencecoral', 'diorite_wall', 'substitute', with_cover=True)
    for kind in ['papyrus', 'tule', 'coopersreed', 'brownsedge']:
        add(f'tallplant-{kind}-*', 'sugar_cane', None, 'equivalent' if kind == 'papyrus' else 'substitute',
            'reeds; sugar cane cannot be waterlogged')
    add('attachingplant-lichen', 'glow_lichen', {'down': 'true'}, 'substitute', 'emits light in MC')
    add('attachingplant-moss', 'moss_carpet', None, 'substitute')
    add('attachingplant-spottymoss', 'moss_carpet', None, 'substitute')
    removed('attachingplant-*', 'mold/barnacles/dead fungi')
    add('mosslayertinted-*', 'moss_carpet', None, 'equivalent')
    for d in DIRS:
        add(f'wildvine-*-{d}', 'vine', {d: 'true'}, 'equivalent', 'attachment side unverified')
        add(f'wildvinestatic-*-{d}', 'vine', {d: 'true'}, 'equivalent', 'attachment side unverified')
    add('hanginglichen-*-bottom', 'cave_vines', {'berries': 'false', 'age': 25}, 'substitute')
    add('hanginglichen-*-short', 'cave_vines', {'berries': 'false', 'age': 25}, 'substitute')
    add('hanginglichen-*', 'cave_vines_plant', {'berries': 'false'}, 'substitute')
    add('glowworms-end*', 'cave_vines', {'berries': 'true', 'age': 25}, 'substitute', 'glowing')
    add('glowworms-*', 'cave_vines_plant', {'berries': 'true'}, 'substitute', 'glowing')
    add('devgrowth-*', 'dead_bush', None, 'substitute')
    add('devastationgrowth-*', 'dead_bush', None, 'substitute')
    add('locustnest-*', 'air', None, 'removed', 'locust nest decoration')
    for t, col in [('55deg', 'green'), ('65deg', 'yellow'), ('74deg', 'orange'), ('87deg', 'brown')]:
        add(f'hotspringbacteria*-{t}', f'{col}_carpet', None, 'substitute', 'hot spring bacterial mat')
    removed('precipitate-*', 'dust layer')
    add('egg-*', 'air', None, 'removed', 'egg on the ground')

    # ---------------- decorations, machines, everything else without an MC block
    for f in ['overlay-*', 'paint-*', 'oxidation-*', 'caveart-*', 'symbols-*', 'scribbles-*', 'wallscribbles-*',
              'wallcarving-*', 'painting-*', 'tapestry-*', 'banner-*', 'plaque-*', 'figurehead-*', 'antlermount',
              'clutter*', 'mannequin-*', 'toolrack-*', 'moldrack-*', 'shelf-*', 'groundstorage', 'ingotpile',
              'platepile', 'metalpartpile-*', 'metal-*', 'bonyremains-*', 'carcass-*', 'drycarcass-*',
              'bowl-*', 'jug-*', 'crucible-*', 'wateringcan-*', 'ingotmold-*', 'toolmold-*', 'curdbundle',
              'cheese', 'woodbucket', 'sieve-*', 'pan-*', 'knappingsurface', 'clayform', 'clayshattered-*',
              'omoktabletop', 'paper-*', 'cage-*', 'baskettrap-*', 'trapcrate-*', 'ember', 'dpanel-*',
              'doorkiln-*', 'cokeovendoor-*', 'fruitpress*', 'churn-*', 'bellows-*', 'elevatorcontrol-*']:
        removed(f, 'decorative/utility block without MC equivalent (block entity data lost)')
    for f in ['woodenaxle-*', 'woodenaxlehub-*', 'angledgears-*', 'spurgear-*', 'largegear3', 'transmission-*',
              'clutch-*', 'brake-*', 'woodentoggle-*', 'axlearm-*', 'windmillrotor-*', 'waterwheel-*',
              'creativerotor-*', 'pulverizerframe-*', 'mppulverizertop', 'helvehammerbase-*', 'grindingwheel-*',
              'mpmultiblockwood', 'archimedesscrew-*', 'wagonwheels-*', 'jonas*', 'condenser-*',
              'verticalboiler-*', 'statictranslocator-*']:
        removed(f, 'mechanical power / machine part')
    return R


def stairs(add, prefix, mc, q='equivalent'):
    for vh, half in [('up', 'bottom'), ('down', 'top')]:
        for d in DIRS:
            add(f'{prefix}-{vh}-{d}-*', mc, {'facing': d, 'half': half, 'shape': 'straight'}, q)


def slabs(add, prefix, slab, full, full_props=None, q='equivalent'):
    add(f'{prefix}-down-*', slab, {'type': 'bottom'}, q)
    add(f'{prefix}-up-*', slab, {'type': 'top'}, q)
    add(f'{prefix}-*', full, full_props, 'substitute', 'vertical slab: MC has none, full block')


def conn_props(conn, true='true', false='false'):
    c = '' if conn == 'empty' else conn
    return {'north': true if 'n' in c else false, 'east': true if 'e' in c else false,
            'south': true if 's' in c else false, 'west': true if 'w' in c else false}


def fences(add, prefix, mc, q='equivalent'):
    for c in CONNS:
        add(f'{prefix}-{c}-*', mc, conn_props(c), q)


def walls(add, prefix, mc, q='equivalent', with_cover=True):
    for c in CONNS:
        p = conn_props(c, 'low', 'none')
        p['up'] = 'false' if c in ('ns', 'ew') else 'true'
        add(f'{prefix}-{c}-*', mc, p, q)


def gates(add, prefix, mc, q='equivalent', hinge=True):
    for a, facing in [('n', 'north'), ('w', 'east')]:
        for st, op in [('closed', 'false'), ('opened', 'true')]:
            add(f'{prefix}-{a}-{st}-*', mc, {'facing': facing, 'open': op, 'in_wall': 'false'}, q,
                'VS gate axis n/w; open direction and hinge side not mapped')


def fences_iron(add):
    for part in ['base', 'top']:
        for c in ['empty', 'n', 'e', 's', 'w', 'ne', 'ns', 'nw', 'es', 'ew', 'sw', 'nes', 'new', 'nsw', 'esw',
                  'nesw', 'egw', 'gew', 'gns', 'ngs']:
            add(f'ironfence-{part}-{c}', 'iron_bars', conn_props(c.replace('g', '')), 'equivalent')


# --------------------------------------------------------------------------------------------
# Inversion of mapping.toml
# --------------------------------------------------------------------------------------------

BAD_INVERSE = re.compile(r'infested_|waxed_|petrified_|suspicious_|potted_|_wall_hanging|_hanging_sign|soul_|'
                         r'ender_chest|trapped_chest|shulker|composter|frosted|reinforced|budding|_wool$|'
                         r'_concrete_powder|diamond_block|emerald_block|mushroom_block|mushroom_stem|sponge|'
                         r'dried_kelp|pitcher|torchflower|big_dripleaf|glazed|carved_pumpkin|attached_|'
                         r'eyeblossom|pale_|bone_block|_terracotta$|grindstone|stonecutter|chiseled_bookshelf')


def invert(fwd, mc_states):
    cands = collections.defaultdict(list)
    for i, b in enumerate(fwd['block']):
        q = b.get('quality')
        if q not in ('exact', 'equivalent'):
            continue
        vs = b['vs']
        if '{' in vs or vs == 'game:air' or vs.startswith('game:multiblock-'):
            continue
        mc = b['mc']
        if mc not in mc_states:  # MC ids newer than 1.21.1 (pale oak, eyeblossom...)
            continue
        props = {}
        for k, v in (b.get('when') or {}).items():
            props[k] = v[0] if isinstance(v, list) else v
        if props.get('type') == 'double':
            continue
        pen = 0
        if BAD_INVERSE.search(mc):
            pen += 10
        if mc.endswith('_wood') or mc.endswith('_hyphae'):
            pen += 3
        if props.get('half') == 'upper':
            pen += 3
        cands[vs].append(((0 if q == 'exact' else 1) + pen, i, b, props))
    out = {}
    for vs, cs in cands.items():
        cs.sort(key=lambda x: (x[0], x[1]))
        score, i, b, props = cs[0]
        if score >= 10:
            continue
        out[vs] = Rule(vs, b['mc'], props, b['quality'], 'inverted from mapping.toml', auth=False,
                       origin='inverted')
    return out


# --------------------------------------------------------------------------------------------
# MC validation
# --------------------------------------------------------------------------------------------

def load_mc_states(jar):
    out = {'minecraft:air': {}, 'minecraft:cave_air': {}, 'minecraft:void_air': {}}
    with zipfile.ZipFile(jar) as z:
        for n in z.namelist():
            if not (n.startswith('assets/minecraft/blockstates/') and n.endswith('.json')):
                continue
            j = json.loads(z.read(n))
            props = collections.defaultdict(set)
            for key in j.get('variants', {}):
                for kv in filter(None, key.split(',')):
                    k, v = kv.split('=')
                    props[k].update(v.split('|'))

            def walk(w):
                if not w:
                    return
                for k, v in w.items():
                    if k in ('OR', 'AND'):
                        for x in v:
                            walk(x)
                    else:
                        props[k].update(str(v).split('|'))
            for part in j.get('multipart', []):
                walk(part.get('when'))
            out['minecraft:' + n.rsplit('/', 1)[1][:-5]] = props
    # properties that blockstate files do not list (rendered by block entities, or not visual)
    B = ['false', 'true']
    N16 = [str(i) for i in range(16)]
    H = ['north', 'east', 'south', 'west']
    for mc, props in out.items():
        name = mc.split(':')[1]
        for k, v in props.items():
            if v and v <= set(B):  # multipart blockstates often only list the "true" case
                props[k] = set(B)
        props.setdefault('waterlogged', set(B))
        if name.endswith('_leaves'):
            props['distance'] = {str(i) for i in range(1, 8)}
            props['persistent'] = set(B)
        if name in ('water', 'lava'):
            props['level'] = set(N16)
        if name.endswith('_sign') and '_wall_' not in name and 'hanging' not in name:
            props['rotation'] = set(N16)
        if name.endswith('_wall_sign'):
            props['facing'] = set(H)
        if name.endswith('_bed'):
            props.update(facing=set(H), part={'head', 'foot'}, occupied=set(B))
        if name in ('chest', 'trapped_chest'):
            props.update(facing=set(H), type={'single', 'left', 'right'})
        if name.endswith('_wall') and 'up' in props:
            for d in H:
                props[d] = props.get(d, set()) | {'none'}
            props['up'] = set(B)
        if name == 'decorated_pot':
            props.update(facing=set(H), cracked=set(B))
        if name == 'bamboo':
            props['leaves'] = props['leaves'] | {'none'}
        if name in ('kelp', 'cave_vines'):
            props['age'] = {str(i) for i in range(26)}
        if name in ('cactus', 'sugar_cane'):
            props['age'] = set(N16)
    return out


def validate_rule(r, mc_states):
    errs = []
    if r.mc not in mc_states:
        return [f'unknown MC id {r.mc}']
    st = mc_states[r.mc]
    for k, v in r.props.items():
        if k not in st:
            errs.append(f'{r.mc}: unknown property {k}')
        elif v not in st[k]:
            errs.append(f'{r.mc}: invalid value {k}={v} (valid: {sorted(st[k])})')
    return errs


# --------------------------------------------------------------------------------------------
# Output
# --------------------------------------------------------------------------------------------

HEADER = '''# Odasio REVERSE block mapping: Vintage Story 1.22.7 -> Minecraft Java 1.21.1
# Generated by tools/mapping/gen_reverse.py (do not edit by hand: edit the script tables and
# re-run it; `--check` validates coverage and every MC block state against the 1.21.1 jar).
#
# Schema
# ------
# [default]              # used when no [[block]] rule matches a VS code
#   solid     = MC block for unknown VS blocks that are opaque (light absorption 99, the VS
#               default, see assets/vs_block_absorption.tsv)
#   non_solid = MC block for the other unknown VS blocks
#
# [[block]]              # one rule; rules are evaluated in FILE ORDER, FIRST MATCH WINS
#   vs      = "game:rock-granite"   # full VS code incl. domain, or a glob where `*` matches any
#                                   # sequence of characters (including "-" and the empty string),
#                                   # e.g. "game:ore-*-nativecopper-*". No other glob syntax.
#   mc      = "minecraft:stone"     # MC block id (1.21.1)
#   props   = { axis = "y" }        # optional MC blockstate properties (strings). Valid for 1.21.1;
#                                   # missing properties take the MC default state.
#   quality = "exact"               # exact      : same block
#                                   # equivalent : same role, MC variant (species, colour...)
#                                   # substitute : another block standing in
#                                   # removed    : no MC equivalent (air)
#   note    = "..."                 # optional free text
#
# Order: inverted exact codes (from assets/mapping.toml), then per-family rules: exact codes
# first, then specific globs, then broad globs. Liquids (water-*, saltwater-*, lava-*...) are
# listed even though VS keeps them in a separate fluid layer: the converter has to merge the
# fluid layer into the block layer (water in air -> water, water in a waterloggable block ->
# waterlogged=true, otherwise keep the block).
'''


def tstr(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'


def write(rules, path):
    lines = [HEADER, '[default]', 'solid = "minecraft:stone"', 'non_solid = "minecraft:air"', '']
    for r in rules:
        lines.append('[[block]]')
        lines.append(f'vs = {tstr(r.vs)}')
        lines.append(f'mc = {tstr(r.mc)}')
        if r.props:
            lines.append('props = { ' + ', '.join(f'{k} = {tstr(v)}' for k, v in r.props.items()) + ' }')
        lines.append(f'quality = {tstr(r.quality)}')
        if r.note:
            lines.append(f'note = {tstr(r.note)}')
        lines.append('')
    with open(path, 'w') as f:
        f.write('\n'.join(lines))


def load_rules(path):
    d = toml_reader.load(open(path, 'rb'))
    return d['default'], [Rule(b['vs'], b['mc'], b.get('props'), b['quality'], b.get('note'), origin='file')
                          for b in d['block']]


class Matcher:
    """First-match evaluation with a bucket index on the first code segment."""

    def __init__(self, rules):
        self.rules = rules
        self.buckets = collections.defaultdict(list)
        self.generic = []
        for i, r in enumerate(rules):
            head = re.split(r'[-*]', r.vs[5:], 1)[0]
            star = '*' in r.vs and r.vs[5:].index('*') <= len(head) if '*' in r.vs else False
            if star and not r.vs[5:].startswith(head + '-') and not r.vs[5:] == head:
                # glob touching the first segment (e.g. "game:bonysoil*"): put in generic list
                self.generic.append(i)
            else:
                self.buckets[head].append(i)

    def first(self, code):
        head = re.split(r'-', code[5:], 1)[0]
        cand = sorted(self.buckets.get(head, []) + self.generic)
        for i in cand:
            if self.rules[i].match(code):
                return i
        return None


def check(default, rules, codes, absorption, mc_states, verbose=True):
    ok = True
    errs = []
    for r in rules:
        for e in validate_rule(r, mc_states):
            errs.append(f'{r.vs}: {e}')
    for k in ('solid', 'non_solid'):
        if default[k] not in mc_states:
            errs.append(f'[default] {k}: unknown MC id {default[k]}')
    if errs:
        ok = False
        print(f'INVALID MC STATES: {len(errs)}')
        for e in errs[:60]:
            print('  ', e)
    m = Matcher(rules)
    used = collections.Counter()
    stats = collections.Counter()
    by_quality = collections.Counter()
    default_codes = []
    for c in sorted(codes):
        i = m.first(c)
        if i is None:
            kind = 'solid' if absorption.get(c, 99) >= 99 else 'non_solid'
            stats['default_' + kind] += 1
            default_codes.append((c, default[kind]))
            continue
        used[i] += 1
        r = rules[i]
        stats['exact' if '*' not in r.vs else 'glob'] += 1
        by_quality[r.quality] += 1
    dead = [r for i, r in enumerate(rules) if not used[i]]
    total = len(codes)
    ndef = stats['default_solid'] + stats['default_non_solid']
    print(f'VS codes: {total}  rules: {len(rules)} ({sum(1 for r in rules if "*" not in r.vs)} exact codes, '
          f'{sum(1 for r in rules if "*" in r.vs)} globs)')
    print(f'  matched by an exact-code rule : {stats["exact"]:6d} ({100 * stats["exact"] / total:.1f} %)')
    print(f'  matched by a glob rule        : {stats["glob"]:6d} ({100 * stats["glob"] / total:.1f} %)')
    print(f'  [default] solid / non_solid   : {stats["default_solid"]} / {stats["default_non_solid"]} '
          f'({100 * ndef / total:.2f} %)')
    print('  by quality (matched codes)    : ' + ', '.join(f'{k} {v}' for k, v in by_quality.most_common()))
    print(f'  rules matching no code (dead or shadowed): {len(dead)}')
    if verbose:
        for r in dead[:40]:
            print('     dead:', r.vs, '->', r.mc)
        fams = collections.Counter(c[5:].split('-')[0] for c, _ in default_codes)
        if fams:
            print('  default by family:', ', '.join(f'{k} {v}' for k, v in fams.most_common()))
    if ndef / total > 0.03:
        print('WARNING: more than 3 % of the codes fall to [default]')
    return ok, stats, by_quality, dead, m


# --------------------------------------------------------------------------------------------

def creative_colours(mc_jar, codes):
    """creativeblock-N / creativeglow-N -> nearest concrete colour (needs Pillow + VS assets)."""
    out = {}
    try:
        from PIL import Image
    except ImportError:
        return out
    tex = os.path.join(VS_ASSETS, 'creative', 'textures', 'block', 'creative')
    if not os.path.isdir(tex):
        return out

    def avg(im):
        px = [p for p in im.convert('RGBA').getdata() if p[3] > 128] or [(255, 255, 255, 255)]
        return tuple(sum(p[i] for p in px) / len(px) for i in range(3))
    with zipfile.ZipFile(mc_jar) as z:
        conc = {c: avg(Image.open(z.open(f'assets/minecraft/textures/block/{c}_concrete.png'))) for c in CONCRETE}
    for c in codes:
        mm = re.match(r'game:(creativeblock|creativeglow)-(\d+)$', c)
        if not mm:
            continue
        f = os.path.join(tex, f'col{mm.group(2)}.png')
        if not os.path.exists(f):
            continue
        col = avg(Image.open(f))
        out[c] = min(CONCRETE, key=lambda k: sum((a - b) ** 2 for a, b in zip(col, conc[k])))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--check', action='store_true', help='only check the existing output file')
    ap.add_argument('--out', default=OUT)
    ap.add_argument('--mc-jar', default=MC_JAR)
    ap.add_argument('--sample', action='store_true', help='print the result for common terrain codes')
    args = ap.parse_args()

    codes = [l.strip() for l in open(os.path.join(ASSETS, 'vs_block_codes.txt')) if l.strip()]
    codeset = set(codes)
    absorption = {}
    for l in open(os.path.join(ASSETS, 'vs_block_absorption.tsv')):
        if l.startswith('#') or not l.strip():
            continue
        c, v = l.rstrip('\n').split('\t')[:2]
        absorption[c] = int(v)
    mc_states = load_mc_states(args.mc_jar)

    if not args.check:
        light = {}
        for l in open(os.path.join(ASSETS, 'vs_block_light.tsv')):
            if l.startswith('#') or not l.strip():
                continue
            p = l.rstrip('\n').split('\t')
            light[p[0]] = int(p[3])
        crop_max = collections.defaultdict(int)
        mushrooms = set()
        rocks = set()
        for c in codes:
            p = c[5:].split('-')
            if p[0] == 'crop':
                crop_max[p[1]] = max(crop_max[p[1]], int(p[2]))
            if p[0] == 'mushroom':
                mushrooms.add(p[1])
            if p[0] in ('rock', 'crackedrock'):
                rocks.add(p[1])
        ctx = dict(codes=codeset, light=light, crop_max=crop_max, mushrooms=sorted(mushrooms),
                   rocks=sorted(rocks), creative_colours=creative_colours(args.mc_jar, codes))
        fam = family_rules(ctx)
        # drop family rules that match no code at all (tables are generous)
        fm = Matcher(fam)
        first_family = {c: fm.first(c) for c in codes}
        live = set(i for i in first_family.values() if i is not None)
        fwd = toml_reader.load(open(os.path.join(ASSETS, 'mapping.toml'), 'rb'))
        inv = invert(fwd, mc_states)
        kept, dropped_auth, same, disagree = [], 0, 0, []
        for vs, r in sorted(inv.items()):
            if vs not in codeset:
                continue
            i = first_family.get(vs)
            if i is not None:
                f = fam[i]
                if f.key() == r.key() or (f.mc == r.mc and all(f.props.get(k) == v for k, v in r.props.items())):
                    same += 1
                    continue
                if f.auth:
                    dropped_auth += 1
                    disagree.append((vs, r, f))
                    continue
            if validate_rule(r, mc_states):
                continue
            kept.append(r)
        print(f'inversion: {len(inv)} candidates, {same} identical to family rules, '
              f'{dropped_auth} overridden by authoritative family rules, {len(kept)} kept')
        if os.environ.get('SHOW_DISAGREE'):
            for vs, r, f in disagree:
                print(f'   {vs}: inverted {r.mc}{r.props} | family {f.mc}{f.props}')
        fam_live = [r for i, r in enumerate(fam) if i in live]
        exact_first = [r for r in fam_live if '*' not in r.vs]
        globs = [r for r in fam_live if '*' in r.vs]
        rules = kept + exact_first + globs
        write(rules, args.out)
        print(f'wrote {args.out}')

    default, rules = load_rules(args.out)
    ok, stats, byq, dead, m = check(default, rules, codes, absorption, mc_states)
    if args.sample:
        for c in ['game:rock-granite', 'game:rock-basalt', 'game:soil-medium-normal', 'game:soil-low-none',
                  'game:forestfloor-3', 'game:log-grown-pine-ud', 'game:leaves-grown3-oak',
                  'game:plankstairs-oak-up-north-free', 'game:plankslab-pine-down-free',
                  'game:ore-medium-nativecopper-granite', 'game:ore-poor-cassiterite-slate',
                  'game:ore-bituminouscoal-shale', 'game:water-still-7', 'game:water-n-3', 'game:snowlayer-2',
                  'game:tallgrass-tall-free', 'game:flower-catmint-free', 'game:woodenfence-oak-nes-free',
                  'game:drystonefence-granite-ns-free', 'game:glasspane-leaded-oak-ns',
                  'game:door-solid-oak', 'game:multiblock-monolithic-0-p1-0', 'game:sign-wall-north',
                  'game:bed-wood-head-north', 'game:claybricks-uneven-four-running-red', 'game:daub-tan-normal',
                  'game:slantedroofing-redclay-north-free', 'game:crop-spelt-5']:
            i = m.first(c)
            r = rules[i] if i is not None else None
            print(f'  {c:45s} -> {r.mc + str(r.props) if r else "[default]"}')
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
