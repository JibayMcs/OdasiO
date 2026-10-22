#!/usr/bin/env python3
"""Generate odasio assets/mapping.toml (Minecraft Java 1.18-1.21 -> Vintage Story 1.22.7).

Usage: python3 -I genmap.py <vs_block_codes.txt> <vs_entity.json> <mc_blocks.json> <out mapping.toml> <stats.json>
Every VS code emitted is validated against vs_block_codes.txt. {rock} templates are
validated for every VS rock type; the set of rocks valid everywhere is reported.
"""
import json, sys, re, itertools

codes = set(l.strip() for l in open(sys.argv[1]) if l.strip())
entity = json.load(open(sys.argv[2]))
mcblocks = json.load(open(sys.argv[3]))
OUT, STATS = sys.argv[4], sys.argv[5]

# MC blocks added after 1.21.1 (not in the 1.21.1 jar we extracted)
EXTRA_MC = {
    # 1.21.4 Winter Drop / 1.21.5 / 1.21.6
    'pale_oak_log': {'axis': ['x', 'y', 'z']}, 'pale_oak_wood': {'axis': ['x', 'y', 'z']},
    'stripped_pale_oak_log': {'axis': ['x', 'y', 'z']}, 'stripped_pale_oak_wood': {'axis': ['x', 'y', 'z']},
    'pale_oak_planks': {}, 'pale_oak_leaves': {'persistent': ['false', 'true']}, 'pale_oak_sapling': {},
    'pale_oak_slab': {}, 'pale_oak_stairs': {}, 'pale_oak_fence': {}, 'pale_oak_fence_gate': {},
    'pale_oak_door': {}, 'pale_oak_trapdoor': {}, 'pale_oak_button': {}, 'pale_oak_pressure_plate': {},
    'pale_oak_sign': {}, 'pale_oak_wall_sign': {}, 'pale_oak_hanging_sign': {}, 'pale_oak_wall_hanging_sign': {},
    'potted_pale_oak_sapling': {}, 'pale_moss_block': {}, 'pale_moss_carpet': {}, 'pale_hanging_moss': {},
    'open_eyeblossom': {}, 'closed_eyeblossom': {}, 'potted_open_eyeblossom': {}, 'potted_closed_eyeblossom': {},
    'creaking_heart': {}, 'resin_block': {}, 'resin_bricks': {}, 'resin_brick_slab': {}, 'resin_brick_stairs': {},
    'resin_brick_wall': {}, 'chiseled_resin_bricks': {}, 'resin_clump': {},
    'bush': {}, 'firefly_bush': {}, 'short_dry_grass': {}, 'tall_dry_grass': {}, 'cactus_flower': {},
    'leaf_litter': {}, 'wildflowers': {}, 'test_block': {}, 'test_instance_block': {}, 'dried_ghast': {},
    # legacy id (pre 1.20.3) for short_grass
    'grass': {},
}
for k, v in EXTRA_MC.items():
    mcblocks.setdefault('minecraft:' + k, v)

ROCKS = sorted(c[len('game:rock-'):] for c in codes if re.fullmatch(r'game:rock-[a-z]+', c))
DEFAULT_ROCK = 'granite'

entries = []      # list of dicts
errors = []
template_codes = set()


def valid(vs):
    if '{rock}' in vs:
        template_codes.add(vs)
        return vs.replace('{rock}', DEFAULT_ROCK) in codes
    return vs in codes


def pick(*cands):
    for c in cands:
        if valid(c):
            return c
    errors.append('none of %r exist' % (cands,))
    return cands[0]


def add(mc, vs, when=None, approx=False, note=None, fluid=None):
    mc = mc if ':' in mc else 'minecraft:' + mc
    if not valid(vs):
        errors.append('%s -> %s (when %s): unknown VS code' % (mc, vs, when))
    e = {'mc': mc, 'vs': vs}
    if when:
        e['when'] = when
    if approx:
        e['approx'] = True
    if fluid:
        e['fluid'] = fluid
    be = entity.get(vs.replace('{rock}', DEFAULT_ROCK))
    if be:
        e['be'] = be
    if note:
        e['note'] = note
    entries.append(e)


AIR = 'game:air'
WATER = 'game:water-still-7'
AXIS = {'y': 'ud', 'z': 'ns', 'x': 'we'}
HFACE = ['north', 'east', 'south', 'west']
OPP = {'north': 'south', 'south': 'north', 'east': 'west', 'west': 'east'}
COLORS = ['white', 'orange', 'magenta', 'light_blue', 'yellow', 'lime', 'pink', 'gray', 'light_gray',
          'cyan', 'purple', 'blue', 'brown', 'green', 'red', 'black']


def axis_entries(mc, fmt, approx=False, note=None):
    for a, s in AXIS.items():
        add(mc, fmt.format(s), {'axis': a}, approx, note)


def slab_entries(mc, fmt, double, approx=False, note=None):
    """fmt has {rot}; VS slab rot: down = bottom slab, up = top slab (no -up-snow variant)."""
    add(mc, fmt.format(rot='down'), {'type': 'bottom'}, approx, note)
    add(mc, fmt.format(rot='up'), {'type': 'top'}, approx, note)
    add(mc, double, {'type': 'double'}, approx, note)


def stairs_entries(mc, fmt, approx=False, note=None):
    """fmt has {vert} {face}. VS 'up' = normal, 'down' = upside-down. VS facing == MC facing
    (high side of the step on the facing side, checked against block/basic/stairs shapes)."""
    n = 'stairs `shape` (inner/outer corners) is not represented in VS, corners become straight stairs'
    if note:
        n = note + '; ' + n
    for half, vert in (('bottom', 'up'), ('top', 'down')):
        for f in HFACE:
            add(mc, fmt.format(vert=vert, face=f), {'half': half, 'facing': f}, approx, n)


def conn_entries(mc, fmt, approx=False, note=None, wall=False):
    """Fences / walls: VS encodes connections as letters in n,e,s,w order or 'empty'.
    MC fences use true/false; MC walls (1.16+) use none/low/tall."""
    for combo in itertools.product([False, True], repeat=4):
        letters = ''.join(d[0] for d, on in zip(HFACE, combo) if on) or 'empty'
        if wall:
            when = {d: (['low', 'tall'] if on else 'none') for d, on in zip(HFACE, combo)}
        else:
            when = {d: ('true' if on else 'false') for d, on in zip(HFACE, combo)}
        add(mc, fmt.format(conn=letters), when, approx, note)


# ---------------------------------------------------------------- air & technical
for b in ['air', 'cave_air', 'void_air', 'structure_void', 'barrier', 'light', 'moving_piston',
          'piston_head', 'nether_portal', 'end_portal', 'end_gateway', 'jigsaw', 'structure_block',
          'test_block', 'test_instance_block']:
    add(b, AIR, approx=b not in ('air', 'cave_air', 'void_air'),
        note=None if b in ('air', 'cave_air', 'void_air') else 'technical / no VS equivalent, removed')
add('bedrock', 'game:mantle', note='VS mantle is the unbreakable bottom layer (hard-coded block, no json)')
for b in ['command_block', 'chain_command_block', 'repeating_command_block']:
    add(b, 'game:commandblock-normal', approx=True, note='command contents not converted')

# ---------------------------------------------------------------- liquids
for liq, vsl in (('water', 'water'), ('lava', 'lava')):
    add(liq, 'game:%s-still-7' % vsl, {'level': '0'}, note='source block')
    for lvl in range(1, 8):
        add(liq, 'game:%s-still-%d' % (vsl, max(1, 7 - lvl)), {'level': str(lvl)}, approx=True,
            note='flowing liquid: MC flow direction not derivable per-block, placed as still liquid of decreasing height')
    add(liq, 'game:%s-d-7' % vsl, {'level': [str(i) for i in range(8, 16)]}, note='falling liquid')
add('bubble_column', WATER, approx=True)
for b in ['water_cauldron', 'cauldron', 'lava_cauldron', 'powder_snow_cauldron']:
    add(b, 'game:barrel', approx=True, note='cauldron -> barrel (contents not converted)')

# ---------------------------------------------------------------- terrain
add('grass_block', 'game:soil-medium-normal', note='VS soil = soil-<fertility>-<grass coverage>; MC snowy ignored')
add('grass', 'game:tallgrass-medium-free', note='pre-1.20.3 id of short_grass')
add('dirt', 'game:soil-medium-none')
add('coarse_dirt', 'game:soil-low-none', approx=True)
add('rooted_dirt', 'game:soil-low-verysparse', approx=True)
add('podzol', 'game:forestfloor-4', approx=True, note='forest floor (leaf-litter covered soil)')
add('mycelium', 'game:soil-low-sparse', approx=True)
add('moss_block', 'game:soil-high-normal', approx=True)
add('pale_moss_block', 'game:soil-high-sparse', approx=True)
add('mud', 'game:soil-compost-none', approx=True, note='no mud in VS; dark compost soil')
add('muddy_mangrove_roots', 'game:soil-compost-none', approx=True)
add('packed_mud', 'game:mudbrick-dark', approx=True)
add('mud_bricks', 'game:mudbrick-light')
slab_entries('mud_brick_slab', 'game:mudbrickslab-light-{rot}-free', 'game:mudbrick-light', approx=False)
add('mud_brick_stairs', 'game:mudbrick-light', approx=True, note='no mud brick stairs in VS')
add('mud_brick_wall', 'game:mudbrick-light', approx=True, note='no mud brick wall in VS')
add('dirt_path', 'game:packeddirt', approx=True)
for m in range(8):
    add('farmland', 'game:farmland-%s-medium' % ('dry' if m == 0 else 'moist'), {'moisture': str(m)})
add('clay', 'game:rawclay-blue-none', note='VS blue clay; fire/red clay also exist')
add('sand', 'game:sand-sandstone', note='VS sand is rock-typed; sandstone sand is the yellow one')
add('red_sand', 'game:sand-bauxite', approx=True, note='bauxite sand is reddish')
add('suspicious_sand', 'game:sand-sandstone', approx=True)
add('gravel', 'game:gravel-{rock}', note='rock-typed gravel follows [settings].rock')
add('suspicious_gravel', 'game:gravel-{rock}', approx=True)
add('soul_sand', 'game:sand-basalt', approx=True)
add('soul_soil', 'game:soil-verylow-none', approx=True)
for i in range(1, 8):
    add('snow', 'game:snowlayer-%d' % i, {'layers': str(i)})
add('snow', 'game:snowblock', {'layers': '8'})
add('snow_block', 'game:snowblock')
add('powder_snow', 'game:snowblock', approx=True)
add('ice', 'game:lakeice')
add('frosted_ice', 'game:lakeice', approx=True)
add('packed_ice', 'game:packedglacierice')
add('blue_ice', 'game:glacierice', approx=True)
add('obsidian', 'game:rock-obsidian')
add('crying_obsidian', 'game:rock-obsidian', approx=True)
add('magma_block', 'game:rock-scoria', approx=True)
add('netherrack', 'game:rock-scoria', approx=True)
add('end_stone', 'game:rock-chalk', approx=True)
add('bone_block', 'game:rock-chalk', approx=True)
add('sponge', 'game:hay-normal-ud', approx=True)
add('wet_sponge', 'game:hay-aged-ud', approx=True)
add('dried_kelp_block', 'game:hay-aged-ud', approx=True)
axis_entries('hay_block', 'game:hay-normal-{}')
add('sculk', 'game:soil-verylow-none', approx=True)
add('sculk_catalyst', 'game:soil-verylow-none', approx=True)
for b in ['sculk_vein', 'sculk_sensor', 'calibrated_sculk_sensor', 'sculk_shrieker']:
    add(b, AIR, approx=True)
add('crimson_nylium', 'game:rock-scoria', approx=True)
add('warped_nylium', 'game:rock-scoria', approx=True)

# ---------------------------------------------------------------- stones
# family helpers: (mc block prefix) -> vs rock, approx
add('stone', 'game:rock-{rock}', note='default rock type comes from [settings].rock')
add('infested_stone', 'game:rock-{rock}', approx=True)
add('smooth_stone', 'game:rockpolished-{rock}')
slab_entries('smooth_stone_slab', 'game:polishedrockslab-{{rock}}-{rot}-free', 'game:rockpolished-{rock}')
slab_entries('stone_slab', 'game:polishedrockslab-{{rock}}-{rot}-free', 'game:rock-{rock}', approx=True)
stairs_entries('stone_stairs', 'game:cobblestonestairs-{{rock}}-{vert}-{face}-free', approx=True,
               note='no raw-rock stairs in VS, cobblestone stairs used')
add('stone_button', AIR, approx=True)
add('stone_pressure_plate', AIR, approx=True)
add('cobblestone', 'game:cobblestone-{rock}')
add('infested_cobblestone', 'game:cobblestone-{rock}', approx=True)
slab_entries('cobblestone_slab', 'game:cobblestoneslab-{{rock}}-{rot}-free', 'game:cobblestone-{rock}')
stairs_entries('cobblestone_stairs', 'game:cobblestonestairs-{{rock}}-{vert}-{face}-free')
conn_entries('cobblestone_wall', 'game:drystonefence-{{rock}}-{conn}-free', approx=True, wall=True,
             note='VS dry stone wall; MC wall `up`/tall states ignored')
add('mossy_cobblestone', 'game:mossycobblestone-{rock}')
slab_entries('mossy_cobblestone_slab', 'game:cobblestoneslab-{{rock}}-{rot}-free', 'game:mossycobblestone-{rock}', approx=True)
stairs_entries('mossy_cobblestone_stairs', 'game:cobblestonestairs-{{rock}}-{vert}-{face}-free', approx=True)
conn_entries('mossy_cobblestone_wall', 'game:drystonefence-{{rock}}-{conn}-free', approx=True, wall=True)
add('stone_bricks', 'game:stonebricks-{rock}')
add('infested_stone_bricks', 'game:stonebricks-{rock}', approx=True)
slab_entries('stone_brick_slab', 'game:stonebrickslab-{{rock}}-{rot}-free', 'game:stonebricks-{rock}')
stairs_entries('stone_brick_stairs', 'game:stonebrickstairs-{{rock}}-{vert}-{face}-free')
conn_entries('stone_brick_wall', 'game:drystonefence-{{rock}}-{conn}-free', approx=True, wall=True)
add('mossy_stone_bricks', 'game:mossystonebricks-{rock}')
add('infested_mossy_stone_bricks', 'game:mossystonebricks-{rock}', approx=True)
slab_entries('mossy_stone_brick_slab', 'game:stonebrickslab-{{rock}}-{rot}-free', 'game:mossystonebricks-{rock}', approx=True)
stairs_entries('mossy_stone_brick_stairs', 'game:stonebrickstairs-{{rock}}-{vert}-{face}-free', approx=True)
conn_entries('mossy_stone_brick_wall', 'game:drystonefence-{{rock}}-{conn}-free', approx=True, wall=True)
add('cracked_stone_bricks', 'game:crackedstonebricks-{rock}')
add('infested_cracked_stone_bricks', 'game:crackedstonebricks-{rock}', approx=True)
add('chiseled_stone_bricks', 'game:agedstonebricks-{rock}', approx=True)
add('infested_chiseled_stone_bricks', 'game:agedstonebricks-{rock}', approx=True)

# named MC stones -> fixed VS rock
NAMED = [
    # mc base, vs rock, approx
    ('granite', 'granite', False), ('andesite', 'andesite', False), ('diorite', 'whitemarble', True),
    ('tuff', 'tuff', True), ('calcite', 'chalk', True), ('dripstone_block', 'limestone', True),
    ('deepslate', 'slate', True), ('blackstone', 'basalt', True), ('basalt', 'basalt', False),
    ('sandstone', 'sandstone', False), ('red_sandstone', 'sandstone', True),
    ('prismarine', 'greenmarble', True), ('purpur', 'redmarble', True), ('end_stone', 'chalk', True),
]


def rock_or(fam, rock, fallback_rock='{rock}'):
    c = 'game:%s-%s' % (fam, rock)
    if c in codes:
        return c, rock
    return 'game:%s-%s' % (fam, fallback_rock), fallback_rock


def stone_family(prefix, rock, approx, block=None, slab=None, stairs=None, wall=None,
                 fam_block='rock', fam_slab='polishedrockslab', fam_stairs='cobblestonestairs', fam_wall='drystonefence'):
    """emit block/slab/stairs/wall for a MC stone family using a fixed VS rock, with fallbacks."""
    def fam_code(fam, r, tail=''):
        for rr in (r, 'basalt' if r in ('scoria', 'tuff', 'obsidian') else r, DEFAULT_ROCK):
            c = 'game:%s-%s%s' % (fam, rr, tail)
            if valid(c):
                return c, rr != r
        return 'game:%s-%s%s' % (fam, r, tail), True
    if block:
        c, fb = fam_code(fam_block, rock)
        add(block, c, approx=approx or fb)
    if slab:
        d, _ = fam_code(fam_block, rock)
        c, fb = fam_code(fam_slab, rock, '-down-free')
        base = c[:-len('-down-free')]
        slab_entries(slab, base + '-{rot}-free', d, approx=approx or fb)
    if stairs:
        c, fb = fam_code(fam_stairs, rock, '-up-north-free')
        base = c[:-len('-up-north-free')]
        stairs_entries(stairs, base + '-{vert}-{face}-free', approx=approx or fb)
    if wall:
        c, fb = fam_code(fam_wall, rock, '-empty-free')
        base = c[:-len('-empty-free')]
        conn_entries(wall, base + '-{conn}-free', approx=True, wall=True)


for mc, rock, ap in [('granite', 'granite', False), ('andesite', 'andesite', False), ('diorite', 'whitemarble', True)]:
    stone_family(mc, rock, ap, block=mc, slab=mc + '_slab', stairs=mc + '_stairs', wall=mc + '_wall')
    stone_family(mc, rock, ap, block='polished_' + mc, slab='polished_%s_slab' % mc, stairs='polished_%s_stairs' % mc,
                 fam_block='rockpolished', fam_stairs='stonebrickstairs')
add('tuff', 'game:rock-tuff')
stone_family('tuff', 'tuff', True, slab='tuff_slab', stairs='tuff_stairs', wall='tuff_wall')
stone_family('tuff', 'tuff', True, block='polished_tuff', slab='polished_tuff_slab', stairs='polished_tuff_stairs',
             wall='polished_tuff_wall', fam_block='rockpolished')
stone_family('tuff', 'travertine', True, block='tuff_bricks', slab='tuff_brick_slab', stairs='tuff_brick_stairs',
             wall='tuff_brick_wall', fam_block='stonebricks', fam_slab='stonebrickslab', fam_stairs='stonebrickstairs')
stone_family('tuff', 'travertine', True, block='chiseled_tuff_bricks', fam_block='agedstonebricks')
stone_family('tuff', 'tuff', True, block='chiseled_tuff', fam_block='rockpolished')
add('calcite', 'game:rock-chalk', approx=True)
add('dripstone_block', 'game:rock-limestone', approx=True)
add('pointed_dripstone', pick(*sorted(c for c in codes if c.startswith('game:stalagsection-limestone'))), approx=True,
    note='VS speleothem section; direction/thickness not mapped')
# deepslate -> slate
add('deepslate', 'game:rock-slate', approx=True, note='deepslate -> slate (dark layered rock); axis ignored')
add('infested_deepslate', 'game:rock-slate', approx=True)
add('reinforced_deepslate', 'game:mantle', approx=True)
stone_family('deepslate', 'slate', True, block='cobbled_deepslate', slab='cobbled_deepslate_slab',
             stairs='cobbled_deepslate_stairs', wall='cobbled_deepslate_wall', fam_block='cobblestone', fam_slab='cobblestoneslab')
stone_family('deepslate', 'slate', True, block='polished_deepslate', slab='polished_deepslate_slab',
             stairs='polished_deepslate_stairs', wall='polished_deepslate_wall', fam_block='rockpolished', fam_stairs='stonebrickstairs')
stone_family('deepslate', 'slate', True, block='deepslate_bricks', slab='deepslate_brick_slab', stairs='deepslate_brick_stairs',
             wall='deepslate_brick_wall', fam_block='stonebricks', fam_slab='stonebrickslab', fam_stairs='stonebrickstairs')
stone_family('deepslate', 'slate', True, block='deepslate_tiles', slab='deepslate_tile_slab', stairs='deepslate_tile_stairs',
             wall='deepslate_tile_wall', fam_block='stonebricks', fam_slab='stonebrickslab', fam_stairs='stonebrickstairs')
add('cracked_deepslate_bricks', 'game:crackedstonebricks-slate', approx=True)
add('cracked_deepslate_tiles', 'game:crackedstonebricks-slate', approx=True)
add('chiseled_deepslate', 'game:agedstonebricks-slate', approx=True)
# blackstone -> basalt
stone_family('blackstone', 'basalt', True, block='blackstone', slab='blackstone_slab', stairs='blackstone_stairs', wall='blackstone_wall')
add('gilded_blackstone', 'game:rock-basalt', approx=True)
stone_family('blackstone', 'basalt', True, block='polished_blackstone', slab='polished_blackstone_slab',
             stairs='polished_blackstone_stairs', wall='polished_blackstone_wall', fam_block='rockpolished', fam_stairs='stonebrickstairs')
stone_family('blackstone', 'basalt', True, block='polished_blackstone_bricks', slab='polished_blackstone_brick_slab',
             stairs='polished_blackstone_brick_stairs', wall='polished_blackstone_brick_wall',
             fam_block='stonebricks', fam_slab='stonebrickslab', fam_stairs='stonebrickstairs')
add('cracked_polished_blackstone_bricks', 'game:crackedstonebricks-basalt', approx=True)
add('chiseled_polished_blackstone', 'game:agedstonebricks-basalt', approx=True)
add('polished_blackstone_button', AIR, approx=True)
add('polished_blackstone_pressure_plate', AIR, approx=True)
add('basalt', 'game:rock-basalt', note='axis ignored')
add('polished_basalt', 'game:rockpolished-basalt', note='axis ignored')
add('smooth_basalt', 'game:rockpolished-basalt')
# sandstones
for pre, ap in (('', False), ('red_', True)):
    stone_family('sandstone', 'sandstone', ap, block=pre + 'sandstone', slab=pre + 'sandstone_slab',
                 stairs=pre + 'sandstone_stairs', wall=pre + 'sandstone_wall', fam_stairs='stonebrickstairs')
    stone_family('sandstone', 'sandstone', ap, block='smooth_%ssandstone' % pre, slab='smooth_%ssandstone_slab' % pre,
                 stairs='smooth_%ssandstone_stairs' % pre, fam_block='rockpolished', fam_stairs='stonebrickstairs')
    stone_family('sandstone', 'sandstone', True, block='cut_%ssandstone' % pre, slab='cut_%ssandstone_slab' % pre,
                 fam_block='stonebricks', fam_slab='stonebrickslab')
    add('chiseled_%ssandstone' % pre, 'game:agedstonebricks-sandstone', approx=True)
# bricks (clay)
add('bricks', 'game:claybricks-uneven-four-running-red', note='VS fired clay bricks')
slab_entries('brick_slab', 'game:brickslabs-four-red-{rot}-free', 'game:claybricks-uneven-four-running-red')
stairs_entries('brick_stairs', 'game:brickstairs-four-red-{vert}-{face}-free')
conn_entries('brick_wall', 'game:drystonefence-{{rock}}-{conn}-free', approx=True, wall=True,
             note='no brick wall/fence in VS, dry stone wall used')
for mc, col in (('nether_bricks', 'brown'), ('red_nether_bricks', 'red')):
    base = mc[:-1]  # nether_brick
    add(mc, 'game:claybricks-uneven-four-running-%s' % col, approx=True)
    slab_entries(base + '_slab', 'game:brickslabs-four-%s-{rot}-free' % col, 'game:claybricks-uneven-four-running-%s' % col, approx=True)
    stairs_entries(base + '_stairs', 'game:brickstairs-four-%s-{vert}-{face}-free' % col, approx=True)
    conn_entries(base + '_wall', 'game:drystonefence-basalt-{conn}-free', approx=True, wall=True)
add('cracked_nether_bricks', 'game:claybricks-damaged-fire', approx=True)
add('chiseled_nether_bricks', 'game:claybricks-uneven-four-soldier-brown', approx=True)
conn_entries('nether_brick_fence', 'game:drystonefence-basalt-{conn}-free', approx=True)
# end stone bricks, prismarine, purpur, quartz, resin
stone_family('end', 'chalk', True, block='end_stone_bricks', slab='end_stone_brick_slab', stairs='end_stone_brick_stairs',
             wall='end_stone_brick_wall', fam_block='stonebricks', fam_slab='stonebrickslab', fam_stairs='stonebrickstairs')
stone_family('prismarine', 'greenmarble', True, block='prismarine', slab='prismarine_slab', stairs='prismarine_stairs',
             wall='prismarine_wall')
stone_family('prismarine', 'greenmarble', True, block='prismarine_bricks', slab='prismarine_brick_slab',
             stairs='prismarine_brick_stairs', fam_block='rockpolished')
stone_family('prismarine', 'basalt', True, block='dark_prismarine', slab='dark_prismarine_slab',
             stairs='dark_prismarine_stairs', fam_block='rockpolished')
stone_family('purpur', 'redmarble', True, block='purpur_block', slab='purpur_slab', stairs='purpur_stairs',
             fam_block='rockpolished')
add('purpur_pillar', 'game:rockpolished-redmarble', approx=True)
add('quartz_block', 'game:quartz-plain')
add('smooth_quartz', 'game:quartz-plain', approx=True)
add('quartz_bricks', 'game:quartz-plain', approx=True)
add('chiseled_quartz_block', 'game:quartz-ornate')
axis_entries('quartz_pillar', 'game:quartzpillar-{}')
for mc in ('quartz', 'smooth_quartz'):
    slab_entries(mc + '_slab', 'game:quartzslab-{rot}-free', 'game:quartz-plain', approx=mc != 'quartz')
    stairs_entries(mc + '_stairs', 'game:quartzstairs-{vert}-{face}-free', approx=mc != 'quartz')
add('resin_block', 'game:claybricks-uneven-four-running-orange', approx=True)
add('resin_bricks', 'game:claybricks-uneven-four-running-orange', approx=True)
add('chiseled_resin_bricks', 'game:claybricks-uneven-four-soldier-orange', approx=True)
slab_entries('resin_brick_slab', 'game:brickslabs-four-orange-{rot}-free', 'game:claybricks-uneven-four-running-orange', approx=True)
stairs_entries('resin_brick_stairs', 'game:brickstairs-four-orange-{vert}-{face}-free', approx=True)
conn_entries('resin_brick_wall', 'game:drystonefence-sandstone-{conn}-free', approx=True, wall=True)
add('resin_clump', AIR, approx=True)
add('petrified_oak_slab', 'game:plankslab-oak-down-free', {'type': 'bottom'})
add('petrified_oak_slab', 'game:plankslab-oak-up-free', {'type': 'top'})
add('petrified_oak_slab', 'game:planks-oak-ud', {'type': 'double'})

# ---------------------------------------------------------------- ores & mineral blocks
ORES = [
    # mc, vs normal, vs deepslate variant, approx, note
    ('coal_ore', 'game:ore-bituminouscoal-shale', 'game:ore-bituminouscoal-shale', True,
     'VS coal only occurs in sedimentary rock; host rock differs from surrounding stone'),
    ('iron_ore', 'game:ore-medium-hematite-granite', 'game:ore-medium-magnetite-slate', False, None),
    ('copper_ore', 'game:ore-medium-nativecopper-granite', 'game:ore-medium-nativecopper-slate', False, None),
    ('gold_ore', 'game:ore-medium-quartz_nativegold-granite', 'game:ore-medium-quartz_nativegold-slate', False, None),
    ('redstone_ore', 'game:ore-cinnabar-granite', 'game:ore-cinnabar-slate', True, 'redstone -> cinnabar (red mineral)'),
    ('lapis_ore', 'game:ore-lapislazuli-limestone', 'game:ore-lapislazuli-limestone', True, 'VS lapis only in limestone/marbles/bauxite'),
    ('diamond_ore', 'game:ore-medium-diamond-kimberlite', 'game:ore-medium-diamond-kimberlite', False, 'VS diamonds only in kimberlite'),
    ('emerald_ore', 'game:ore-medium-emerald-slate', 'game:ore-medium-emerald-slate', True, None),
]
for mc, n, d, ap, note in ORES:
    add(mc, n, approx=ap, note=note)
    add('deepslate_' + mc, d, approx=True, note=note)
add('nether_gold_ore', 'game:ore-medium-quartz_nativegold-basalt', approx=True)
add('nether_quartz_ore', 'game:ore-quartz-basalt', approx=True)
add('ancient_debris', 'game:ore-medium-ilmenite-basalt' if 'game:ore-medium-ilmenite-basalt' in codes else
    pick(*sorted(c for c in codes if c.startswith('game:ore-medium-ilmenite-'))), approx=True)
add('raw_iron_block', 'game:ore-rich-hematite-granite', approx=True)
add('raw_copper_block', 'game:ore-rich-nativecopper-granite', approx=True)
add('raw_gold_block', 'game:ore-rich-quartz_nativegold-granite', approx=True)
add('iron_block', 'game:metalblock-new-plain-iron')
add('gold_block', 'game:metalblock-new-plain-gold')
add('copper_block', 'game:metalblock-new-plain-copper')
add('netherite_block', 'game:metalblock-new-plain-steel', approx=True)
add('diamond_block', 'game:glass-quartz', approx=True)
add('emerald_block', 'game:glass-green', approx=True)
add('lapis_block', 'game:glass-blue', approx=True)
add('redstone_block', 'game:glass-red', approx=True)
add('coal_block', 'game:coalpile', approx=True, note='VS coal pile (block entity)')
add('amethyst_block', 'game:glass-violet', approx=True)
add('budding_amethyst', 'game:glass-violet', approx=True)
add('amethyst_cluster', 'game:crystal-amethyst-large1-up', approx=True, note='facing not mapped')
add('large_amethyst_bud', 'game:crystal-amethyst-cluster2-up', approx=True)
add('medium_amethyst_bud', 'game:crystal-amethyst-cluster1-up', approx=True)
add('small_amethyst_bud', 'game:crystal-amethyst-small1-up', approx=True)
# copper blocks (all oxidation / waxed states)
for wax in ('', 'waxed_'):
    for ox, metal in (('', 'copper'), ('exposed_', 'copper'), ('weathered_', 'copper'), ('oxidized_', 'copper')):
        corr = 'game:metalblock-corroded-plain-copper' if ox in ('weathered_', 'oxidized_') else 'game:metalblock-new-plain-copper'
        rcorr = corr.replace('-plain-', '-riveted-')
        blk = wax + ox + 'copper' if ox else wax + 'copper_block'
        if ox or wax:
            add(blk, corr, approx=bool(ox))
        add(wax + ox + 'cut_copper', rcorr, approx=True)
        add(wax + ox + 'chiseled_copper', rcorr, approx=True)
        add(wax + ox + 'copper_grate', corr, approx=True)
        slab_entries(wax + ox + 'cut_copper_slab', 'game:plankslab-aged-{rot}-free',
                     rcorr, approx=True, note='no metal slabs in VS')
        stairs_entries(wax + ox + 'cut_copper_stairs', 'game:plankstairs-aged-{vert}-{face}-free', approx=True,
                       note='no metal stairs in VS')
        add(wax + ox + 'copper_bulb', 'game:lantern-small-up', approx=True)
        add(wax + ox + 'copper_door', AIR, {'half': 'upper'}, approx=True, note='VS doors are 2-high from the lower block')
        add(wax + ox + 'copper_door', 'game:door-1x2heavy-aged', approx=True,
            note='VS door orientation/open state live in the block entity (BEBehaviorDoor); converter must create it')
        add(wax + ox + 'copper_trapdoor', 'game:trapdoor-plate-iron-1', approx=True,
            note='trapdoor orientation lives in the block entity')

# ---------------------------------------------------------------- woods
WOODS = [
    # mc, vs wood, vs leaves, vs sapling, approx
    ('oak', 'oak', 'oak', 'oak', False),
    ('spruce', 'pine', 'pine', 'pine', False),
    ('birch', 'birch', 'birch', 'birch', False),
    ('jungle', 'kapok', 'kapok', 'kapok', False),
    ('acacia', 'acacia', 'acacia', 'acacia', False),
    ('dark_oak', 'walnut', 'walnut', 'walnut', False),
    ('mangrove', 'baldcypress', 'baldcypress', None, True),  # sapling = mangrove_propagule
    ('cherry', 'maple', 'crimsonkingmaple', 'crimsonkingmaple', True),
    ('pale_oak', 'aged', 'birch', 'birch', True),
    ('crimson', 'purpleheart', None, None, True),
    ('warped', 'ebony', None, None, True),
    ('bamboo', 'larch', None, None, True),
]
for mc, w, lv, sp, ap in WOODS:
    nether = mc in ('crimson', 'warped')
    if nether:
        axis_entries(mc + '_stem', 'game:log-placed-%s-{}' % w, approx=True)
        axis_entries(mc + '_hyphae', 'game:log-placed-%s-{}' % w, approx=True)
        axis_entries('stripped_%s_stem' % mc, 'game:debarkedlog-%s-{}' % w, approx=True)
        axis_entries('stripped_%s_hyphae' % mc, 'game:debarkedlog-%s-{}' % w, approx=True)
    elif mc == 'bamboo':
        axis_entries('bamboo_block', 'game:stackedbamboo-{}')
        axis_entries('stripped_bamboo_block', 'game:stackedbamboo-{}', approx=True)
        add('bamboo_mosaic', 'game:planks-larch-ud', approx=True)
        slab_entries('bamboo_mosaic_slab', 'game:plankslab-larch-{rot}-free', 'game:planks-larch-ud', approx=True)
        stairs_entries('bamboo_mosaic_stairs', 'game:plankstairs-larch-{vert}-{face}-free', approx=True)
    else:
        lognote = 'VS log-placed (player placed, not part of a tree); log-grown-* is the worldgen tree variant'
        axis_entries(mc + '_log', 'game:log-placed-%s-{}' % w, ap, lognote)
        axis_entries(mc + '_wood', 'game:log-placed-%s-{}' % w, True, 'no 6-sided bark block in VS')
        axis_entries('stripped_%s_log' % mc, 'game:debarkedlog-%s-{}' % w, ap)
        axis_entries('stripped_%s_wood' % mc, 'game:debarkedlog-%s-{}' % w, True)
    add(mc + '_planks', 'game:planks-%s-ud' % w, approx=ap)
    slab_entries(mc + '_slab', 'game:plankslab-%s-{rot}-free' % w, 'game:planks-%s-ud' % w, ap)
    stairs_entries(mc + '_stairs', 'game:plankstairs-%s-{vert}-{face}-free' % w, ap)
    if mc == 'bamboo':
        conn_entries('bamboo_fence', 'game:fencebamboo-brown-{conn}-free' if valid('game:fencebamboo-brown-empty-free')
                     else 'game:woodenfence-larch-{conn}-free', approx=True)
        for f in HFACE:
            for o in ('false', 'true'):
                add('bamboo_fence_gate', 'game:bamboogate-brown-%s-%s-free' % ('n' if f in ('north', 'south') else 'w',
                    'opened' if o == 'true' else 'closed'), {'facing': f, 'open': o}, approx=True,
                    note='VS gate axis n/w; mapping n<-north/south facing is unverified')
    else:
        conn_entries(mc + '_fence', 'game:woodenfence-%s-{conn}-free' % w, ap)
        for f in HFACE:
            for o in ('false', 'true'):
                add(mc + '_fence_gate', 'game:woodenfencegate-%s-%s-%s-left-free' % (
                    w, 'n' if f in ('north', 'south') else 'w', 'opened' if o == 'true' else 'closed'),
                    {'facing': f, 'open': o}, approx=ap,
                    note='VS gate axis n/w; mapping n<-north/south facing is unverified; hinge side not mapped')
    add(mc + '_door', AIR, {'half': 'upper'}, note='VS doors are placed from the lower block only')
    add(mc + '_door', 'game:door-solid-%s' % w, {'half': 'lower'}, approx=ap,
        note='VS 1.18+ door: orientation, hinge and open state are stored in the block entity (BEBehaviorDoor); converter must create it')
    add(mc + '_trapdoor', 'game:trapdoor-solid-%s-1' % w, approx=ap,
        note='VS trapdoor orientation/open state live in the block entity')
    for s in ('_button', '_pressure_plate'):
        add(mc + s, AIR, approx=True, note='no VS equivalent')
    add(mc + '_sign', 'game:sign-ground-north', approx=True, note='16-step rotation and text (block entity) not mapped')
    for f in HFACE:
        add(mc + '_wall_sign', 'game:sign-wall-%s' % f, {'facing': f}, approx=True, note='text lives in block entity')
        add(mc + '_wall_hanging_sign', 'game:sign-wall-%s' % f, {'facing': f}, approx=True)
    add(mc + '_hanging_sign', 'game:sign-ground-north', approx=True)
    if lv:
        add(mc + '_leaves', 'game:leaves-placed-%s' % lv, {'persistent': 'true'}, ap)
        add(mc + '_leaves', 'game:leaves-grown-%s' % lv, {'persistent': 'false'}, ap,
            note='VS leaves-grown-* decay naturally like MC non-persistent leaves')
    if sp:
        add(mc + '_sapling', 'game:sapling-%s-free' % sp, approx=ap)
        add('potted_%s_sapling' % mc, 'game:flowerpot-red-fired', approx=True, note='pot contents (block entity) not mapped')
add('mangrove_propagule', 'game:sapling-baldcypress-free', approx=True)
add('potted_mangrove_propagule', 'game:flowerpot-red-fired', approx=True)
add('mangrove_roots', 'game:log-placed-baldcypress-ud', approx=True)
add('azalea_leaves', 'game:leaves-grown-oak', {'persistent': 'false'}, approx=True)
add('azalea_leaves', 'game:leaves-placed-oak', {'persistent': 'true'}, approx=True)
add('flowering_azalea_leaves', 'game:leaves-grown-oak', {'persistent': 'false'}, approx=True)
add('flowering_azalea_leaves', 'game:leaves-placed-oak', {'persistent': 'true'}, approx=True)
add('azalea', 'game:smallberrybush-blueberry-empty', approx=True)
add('flowering_azalea', 'game:smallberrybush-blueberry-flowering', approx=True)
add('nether_wart_block', 'game:leaves-placed-crimsonkingmaple', approx=True)
add('warped_wart_block', 'game:leaves-placed-ebony', approx=True)
add('shroomlight', 'game:paperlantern-on', approx=True)
for b in ('crimson_fungus', 'warped_fungus', 'crimson_roots', 'warped_roots', 'nether_sprouts'):
    add(b, 'game:tallgrass-short-free', approx=True)
for b in ('weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant'):
    add(b, 'game:hanginglichen-oldmans-segment', approx=True)
for f in HFACE:
    add('ladder', 'game:ladder-wood-oak-%s' % OPP[f], {'facing': f},
        note='VS ladder side = wall it is attached to (opposite of MC facing), checked against ladder-plank shape')
add('scaffolding', 'game:ladder-stick-north', approx=True)
for f in HFACE:
    add('chest', 'game:chest-%s' % f, {'facing': f},
        note='VS chest needs a GenericTypedContainer block entity (type=normal); `type` left/right double chests not merged; orientation equality unverified')
    add('trapped_chest', 'game:chest-%s' % f, {'facing': f}, approx=True)
    add('ender_chest', 'game:chest-%s' % f, {'facing': f}, approx=True)
    add('barrel', 'game:barrel', {'facing': f}, approx=True)
add('barrel', 'game:barrel', approx=True)
add('bookshelf', 'game:bookshelves-ns', approx=True, note='VS bookshelf orientation ns/we; contents not mapped')
add('chiseled_bookshelf', 'game:bookshelves-ns', approx=True)
for b in ('crafting_table', 'cartography_table', 'fletching_table', 'smithing_table', 'loom', 'lectern', 'enchanting_table'):
    add(b, 'game:table-normal', approx=True, note='no crafting station block in VS; decorative table')
add('composter', 'game:barrel', approx=True)
add('beehive', 'game:skep-papyrus-empty-north', approx=True)
add('bee_nest', 'game:wildbeehive-medium-empty' if valid('game:wildbeehive-medium-empty')
    else pick(*sorted(c for c in codes if c.startswith('game:wildbeehive-'))), approx=True)
for f in HFACE:
    for lit in ('false', 'true'):
        for b in ('furnace', 'blast_furnace', 'smoker'):
            add(b, 'game:clayoven-%s' % f, {'facing': f, 'lit': lit}, approx=True, note='VS clay oven (block entity), orientation unverified')
add('campfire', 'game:firepit-lit', {'lit': 'true'}, approx=True)
add('campfire', 'game:firepit-extinct', {'lit': 'false'}, approx=True)
add('soul_campfire', 'game:firepit-lit', {'lit': 'true'}, approx=True)
add('soul_campfire', 'game:firepit-extinct', {'lit': 'false'}, approx=True)
add('fire', 'game:fire', approx=True)
add('soul_fire', 'game:fire', approx=True)
for b in ('anvil', 'chipped_anvil', 'damaged_anvil'):
    add(b, 'game:anvil-iron', approx=True, note='orientation not mapped')
add('grindstone', 'game:quern-granite', approx=True)
add('stonecutter', 'game:quern-granite', approx=True)
add('hopper', 'game:hopper')
add('cobweb', 'game:spiderweb')
add('honeycomb_block', 'game:glass-yellow', approx=True)
conn_entries('iron_bars', 'game:ironfence-base-{conn}', approx=True, note='VS iron fence')
add('chain', pick('game:supportchain-iron', *sorted(c for c in codes if c.startswith('game:supportchain-'))), approx=True)
add('iron_door', AIR, {'half': 'upper'})
for f in HFACE:
    for o in ('false', 'true'):
        for h in ('left', 'right'):
            add('iron_door', 'game:irondoor-%s-down-%s-%s' % (f, 'opened' if o == 'true' else 'closed', h),
                {'half': 'lower', 'facing': f, 'open': o, 'hinge': h}, approx=True,
                note='legacy VS iron door (orientation in code); upper half -> irondoor-...-up-... may be needed, unverified')
add('iron_trapdoor', 'game:trapdoor-bars-iron-1', approx=True)
add('heavy_weighted_pressure_plate', AIR, approx=True)
add('light_weighted_pressure_plate', AIR, approx=True)

# lighting
add('torch', 'game:torch-basic-lit-up')
for f in HFACE:
    add('wall_torch', 'game:torch-basic-lit-%s' % f, {'facing': f},
        note='same facing convention as MC (checked against torch wall shape)')
    add('soul_wall_torch', 'game:torch-basic-lit-%s' % f, {'facing': f}, approx=True)
    add('redstone_wall_torch', 'game:torch-basic-extinct-%s' % f, {'facing': f}, approx=True)
add('soul_torch', 'game:torch-basic-lit-up', approx=True)
add('redstone_torch', 'game:torch-basic-extinct-up', approx=True)
for b in ('lantern', 'soul_lantern'):
    add(b, 'game:lantern-small-down', {'hanging': 'true'}, approx=True, note='lantern material lives in block entity')
    add(b, 'game:lantern-small-up', {'hanging': 'false'}, approx=True)
for b in ('glowstone', 'sea_lantern', 'ochre_froglight', 'verdant_froglight', 'pearlescent_froglight', 'redstone_lamp',
          'beacon', 'jack_o_lantern'):
    add(b, 'game:paperlantern-on', approx=True, note='light-emitting full block approximated by a paper lantern')
add('end_rod', 'game:torch-basic-lit-up', approx=True)
add('candle', 'game:candle', approx=True)
for c in COLORS:
    add(c + '_candle', 'game:candle', approx=True)
    add(c + '_candle_cake', AIR, approx=True)
add('candle_cake', AIR, approx=True)
add('cake', AIR, approx=True)

# glass
add('glass', 'game:glass-plain')
add('tinted_glass', 'game:glass-smoky')
add('glass_pane', 'game:glasspane-leaded-oak-ew', {'east': 'true'}, approx=True, note='VS panes are leaded (wood framed), ns/we only')
add('glass_pane', 'game:glasspane-leaded-oak-ew', {'west': 'true'}, approx=True)
add('glass_pane', 'game:glasspane-leaded-oak-ns', approx=True)
GLASS = {'white': 'plain', 'orange': 'brown', 'magenta': 'violet', 'light_blue': 'blue', 'yellow': 'yellow',
         'lime': 'green', 'pink': 'pink', 'gray': 'smoky', 'light_gray': 'vintage', 'cyan': 'blue',
         'purple': 'violet', 'blue': 'blue', 'brown': 'brown', 'green': 'green', 'red': 'red', 'black': 'smoky'}
for c in COLORS:
    add(c + '_stained_glass', 'game:glass-' + GLASS[c], approx=c not in ('yellow', 'pink', 'blue', 'brown', 'green', 'red'))
    add(c + '_stained_glass_pane', 'game:glasspane-leaded-oak-ew', {'east': 'true'}, approx=True, note='colour lost')
    add(c + '_stained_glass_pane', 'game:glasspane-leaded-oak-ew', {'west': 'true'}, approx=True)
    add(c + '_stained_glass_pane', 'game:glasspane-leaded-oak-ns', approx=True)

# colored blocks
DAUB = {'white': 'tan', 'orange': 'orange', 'magenta': 'pink', 'light_blue': 'blue', 'yellow': 'yellow',
        'lime': 'green', 'pink': 'pink', 'gray': 'ash', 'light_gray': 'ash', 'cyan': 'blue', 'purple': 'pink',
        'blue': 'blue', 'brown': 'brown', 'green': 'green', 'red': 'orange', 'black': 'ash'}
TILE = {'white': 'cream', 'orange': 'orange', 'magenta': 'red', 'light_blue': 'blue', 'yellow': 'tan',
        'lime': 'tan', 'pink': 'red', 'gray': 'gray', 'light_gray': 'gray', 'cyan': 'blue', 'purple': 'blue',
        'blue': 'blue', 'brown': 'brown', 'green': 'gray', 'red': 'red', 'black': 'black'}
WOOL = {'white': 'white', 'light_gray': 'white', 'gray': 'black', 'black': 'black', 'brown': 'brown'}
CARPET = {'white': None, 'orange': 'red', 'magenta': 'purple', 'light_blue': 'turqoise', 'yellow': None,
          'lime': 'turqoise', 'pink': 'red', 'gray': 'black', 'light_gray': None, 'cyan': 'turqoise',
          'purple': 'purple', 'blue': 'blue', 'brown': 'brown', 'green': 'turqoise', 'red': 'red', 'black': 'black'}
for c in COLORS:
    if c in WOOL:
        add(c + '_wool', 'game:wool-%s-up' % WOOL[c], approx=c in ('light_gray', 'gray'))
    else:
        add(c + '_wool', 'game:daub-%s-normal' % DAUB[c], approx=True, note='VS wool only white/black/brown; coloured daub keeps the colour')
    add(c + '_concrete', 'game:daub-%s-normal' % DAUB[c], approx=True)
    add(c + '_concrete_powder', 'game:daub-%s-cracked' % DAUB[c], approx=True)
    add(c + '_terracotta', 'game:tile-2x2-plain-%s' % TILE[c], approx=True, note='fired clay tile block')
    add(c + '_glazed_terracotta', 'game:tile-2x2-plain-%s' % TILE[c], approx=True)
    if CARPET[c]:
        add(c + '_carpet', 'game:smallcarpet-%s' % CARPET[c], approx=True)
    else:
        add(c + '_carpet', AIR, approx=True, note='no matching carpet colour in VS')
    for part, p in (('head', 'head'), ('foot', 'feet')):
        for f in HFACE:
            add(c + '_bed', 'game:bed-wood-%s-%s' % (p, f), {'part': part, 'facing': f}, approx=True,
                note='colour lost; VS bed facing convention unverified')
    add(c + '_banner', AIR, approx=True, note='banner patterns need block entity; dropped')
    add(c + '_wall_banner', AIR, approx=True)
    add(c + '_shulker_box', 'game:chest-north', approx=True, note='contents not converted')
add('shulker_box', 'game:chest-north', approx=True)
add('terracotta', 'game:hardenedclay-fire', approx=True)
add('moss_carpet', 'game:mosslayertinted-full', approx=True)
add('pale_moss_carpet', 'game:mosslayertinted-patchy', approx=True)
add('pale_hanging_moss', 'game:hanginglichen-lace-segment', approx=True)
add('leaf_litter', AIR, approx=True, note='thin decoration, dropped')

# ---------------------------------------------------------------- plants
add('short_grass', 'game:tallgrass-medium-free')
add('tall_grass', 'game:tallgrass-verytall-free', {'half': 'lower'})
add('tall_grass', AIR, {'half': 'upper'}, note='VS tall grass is a single block')
add('fern', 'game:fern-eaglefern')
add('large_fern', 'game:tallfern', {'half': 'lower'})
add('large_fern', AIR, {'half': 'upper'})
add('dead_bush', 'game:tallgrass-eaten-free', approx=True)
add('bush', 'game:tallgrass-tall-free', approx=True)
add('firefly_bush', 'game:tallgrass-tall-free', approx=True)
add('short_dry_grass', 'game:tallgrass-eaten-free', approx=True)
add('tall_dry_grass', 'game:tallgrass-eaten-free', approx=True)
add('hanging_roots', 'game:hanginglichen-oldmans-short', approx=True)
add('spore_blossom', AIR, approx=True)
add('glow_lichen', 'game:attachingplant-lichen', approx=True)
add('vine', 'game:wildvine-section-north', approx=True, note='VS vine has one attachment side; MC multi-face vines collapse to north')
add('sugar_cane', 'game:tallplant-papyrus-land-normal-free', approx=True)
add('cactus', 'game:saguarocactus-segment', approx=True)
add('cactus_flower', 'game:saguarocactus-topflowering', approx=True)
add('lily_pad', 'game:waterlily')
add('big_dripleaf', 'game:aquatic-lilygiant-small', approx=True)
add('big_dripleaf_stem', AIR, approx=True)
add('small_dripleaf', 'game:tallgrass-short-free', approx=True)
add('seagrass', 'game:aquatic-seagrass-green-section', fluid=WATER)
add('tall_seagrass', 'game:aquatic-seagrass-green-section', fluid=WATER)
add('kelp', 'game:aquatic-kelp-top', fluid=WATER)
add('kelp_plant', 'game:aquatic-kelp-section', fluid=WATER)
add('sea_pickle', 'game:coral-anemone-green', approx=True, fluid=WATER)
add('frogspawn', AIR, approx=True)
add('turtle_egg', AIR, approx=True)
add('sniffer_egg', AIR, approx=True)
add('dragon_egg', AIR, approx=True)
add('chorus_plant', 'game:saguarocactus-branchysegment', approx=True)
add('chorus_flower', 'game:saguarocactus-topflowering', approx=True)
FLOWERS = {
    'dandelion': 'goldenpoppy', 'poppy': 'orangemallow', 'blue_orchid': 'forgetmenot', 'allium': 'lupine-purple',
    'azure_bluet': 'edelweiss', 'red_tulip': 'lupine-red', 'orange_tulip': 'orangemallow', 'white_tulip': 'lupine-white',
    'pink_tulip': 'heather', 'oxeye_daisy': 'wilddaisy', 'cornflower': 'cornflower', 'lily_of_the_valley': 'lilyofthevalley',
    'wither_rose': 'ghostpipered', 'torchflower': 'orangemallow', 'open_eyeblossom': 'ghostpipewhite',
    'closed_eyeblossom': 'ghostpipepink', 'pink_petals': 'heather', 'wildflowers': 'wilddaisy',
}
EXACT_FLOWERS = {'cornflower', 'lily_of_the_valley', 'oxeye_daisy'}
for mc, vs in FLOWERS.items():
    add(mc, 'game:flower-%s-free' % vs, approx=mc not in EXACT_FLOWERS)
    if mc not in ('pink_petals', 'wildflowers'):
        add('potted_' + mc, 'game:flowerpot-red-fired', approx=True, note='pot contents (block entity) not mapped')
TALLFLOWERS = {'sunflower': 'goldenpoppy', 'lilac': 'lupine-purple', 'rose_bush': 'lupine-red', 'peony': 'heather',
               'pitcher_plant': 'lupine-blue'}
for mc, vs in TALLFLOWERS.items():
    add(mc, 'game:flower-%s-free' % vs, {'half': 'lower'}, approx=True)
    add(mc, AIR, {'half': 'upper'})
for b in ('potted_fern', 'potted_dead_bush', 'potted_cactus', 'potted_bamboo', 'potted_azalea_bush',
          'potted_flowering_azalea_bush', 'potted_brown_mushroom', 'potted_red_mushroom', 'potted_crimson_fungus',
          'potted_warped_fungus', 'potted_crimson_roots', 'potted_warped_roots', 'flower_pot'):
    add(b, 'game:flowerpot-red-fired', approx=True)
add('brown_mushroom', 'game:mushroom-fieldmushroom-normal', approx=True)
add('red_mushroom', 'game:mushroom-flyagaric-normal', approx=True)
add('brown_mushroom_block', 'game:planks-walnut-ud', approx=True, note='no giant mushrooms in VS')
add('red_mushroom_block', 'game:planks-purpleheart-ud', approx=True)
add('mushroom_stem', 'game:debarkedlog-birch-ud', approx=True)
add('bamboo', 'game:bamboo-placed-green-segment1', approx=True, note='leaves/age not mapped')
add('bamboo_sapling', 'game:sapling-greenbamboo-free', approx=True)
add('pumpkin', 'game:pumpkin-fruit-4')
add('carved_pumpkin', 'game:pumpkin-fruit-4', approx=True)
add('melon', 'game:pumpkin-fruit-4', approx=True)
for b in ('pumpkin_stem', 'melon_stem', 'attached_pumpkin_stem', 'attached_melon_stem'):
    add(b, 'game:pumpkin-vine-1-normal', approx=True)
for a in range(4):
    add('sweet_berry_bush', 'game:smallberrybush-cranberry-%s' % ['empty', 'empty', 'flowering', 'ripe'][a], {'age': str(a)}, approx=True)
add('cocoa', AIR, approx=True)
add('nether_wart', AIR, approx=True)


def crop(mc, vs, ages, approx):
    n = len([c for c in codes if re.fullmatch(r'game:crop-%s-\d+' % vs, c)])
    for a in range(ages):
        stage = 1 + round(a * (n - 1) / (ages - 1))
        add(mc, 'game:crop-%s-%d' % (vs, stage), {'age': str(a)}, approx,
            note='VS crops need farmland block entity below to keep growing' if a == 0 else None)


crop('wheat', 'spelt', 8, True)
crop('carrots', 'carrot', 8, False)
crop('potatoes', 'cassava', 8, True)
crop('beetroots', 'turnip', 4, True)
crop('torchflower_crop', 'sunflower', 2, True)
crop('pitcher_crop', 'flax', 5, True)

# corals (MC: tube=blue, brain=pink, bubble=purple, fire=red, horn=yellow)
CORAL = {'tube': ('blue', 'tube-pink'), 'brain': ('red', 'brain-red'), 'bubble': ('blue', 'branched-purple'),
         'fire': ('red', 'branched-red'), 'horn': ('yellow', 'brain-yellow')}
FAN = {'tube': 'blue', 'brain': 'red', 'bubble': 'purple', 'fire': 'red', 'horn': 'yellow'}
for k, (blockc, plant) in CORAL.items():
    add(k + '_coral_block', 'game:reefblock-' + blockc, approx=True)
    add('dead_%s_coral_block' % k, 'game:reefblock-tan', approx=True)
    add(k + '_coral', pick('game:coral-%s-1' % plant, 'game:coral-%s' % plant), approx=True, fluid=WATER)
    add('dead_%s_coral' % k, pick('game:coral-branched-dead-1', 'game:coral-branched-dead'), approx=True, fluid=WATER)
    for fan in ('_coral_fan', '_coral_wall_fan'):
        add(k + fan, pick('game:coral-fan-%s-1' % FAN[k], 'game:coral-fan-%s' % FAN[k]), approx=True, fluid=WATER)
        add('dead_' + k + fan, pick('game:coral-fan-dead-1', 'game:coral-fan-dead'), approx=True, fluid=WATER)
add('conduit', AIR, approx=True)

# ---------------------------------------------------------------- redstone & misc mechanisms -> removed / approximated
for b in ('redstone_wire', 'repeater', 'comparator', 'lever', 'tripwire', 'tripwire_hook', 'daylight_detector',
          'observer', 'target', 'note_block', 'jukebox', 'piston', 'sticky_piston', 'dispenser', 'dropper', 'crafter',
          'rail', 'powered_rail', 'detector_rail', 'activator_rail', 'tnt', 'slime_block', 'honey_block', 'bell',
          'brewing_stand', 'respawn_anchor', 'lodestone', 'lightning_rod', 'spawner', 'trial_spawner', 'vault',
          'heavy_core', 'decorated_pot', 'end_portal_frame', 'creaking_heart', 'dried_ghast', 'item_frame',
          'glow_item_frame'):
    vs = AIR
    if b in ('note_block', 'jukebox', 'dispenser', 'dropper', 'crafter', 'observer', 'target', 'piston',
             'sticky_piston', 'lodestone', 'respawn_anchor', 'end_portal_frame', 'creaking_heart'):
        vs = 'game:rockpolished-{rock}' if b in ('lodestone', 'respawn_anchor', 'end_portal_frame', 'observer',
                                                  'dispenser', 'dropper', 'crafter', 'piston', 'sticky_piston') else 'game:planks-oak-ud'
    if b in ('rail', 'powered_rail', 'detector_rail', 'activator_rail'):
        vs = AIR
    if b == 'tnt':
        vs = 'game:hay-normal-ud'
    if b in ('slime_block', 'honey_block'):
        vs = 'game:glass-green' if b == 'slime_block' else 'game:glass-yellow'
    if b == 'decorated_pot':
        vs = 'game:storagevessel-red-fired' if valid('game:storagevessel-red-fired') else \
            pick(*sorted(c for c in codes if c.startswith('game:storagevessel-')))
    if b == 'lightning_rod':
        vs = 'game:lightningrod'
    if b == 'bell':
        vs = AIR
    if b == 'creaking_heart':
        vs = 'game:log-placed-aged-ud'
    add(b, vs, approx=True, note='no VS equivalent (redstone/mechanism); placeholder or removed')
for s in ('skeleton', 'wither_skeleton', 'zombie', 'creeper', 'dragon', 'piglin', 'player'):
    add(s + ('_skull' if 'skeleton' in s else '_head'), AIR, approx=True)
    add(s + ('_wall_skull' if 'skeleton' in s else '_wall_head'), AIR, approx=True)

# ---------------------------------------------------------------- write
by_mc = {}
for e in entries:
    by_mc.setdefault(e['mc'], []).append(e)


def tval(v):
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, list):
        return '[' + ', '.join(tval(x) for x in v) + ']'
    return json.dumps(v)


def inline(d):
    return '{ ' + ', '.join('%s = %s' % (k, tval(v)) for k, v in d.items()) + ' }'


# rocks valid for every {rock} template
safe_rocks = [r for r in ROCKS if all(t.replace('{rock}', r) in codes for t in template_codes)]
mc_ids = set(mcblocks)
mapped = set(by_mc)
unmapped = sorted(mc_ids - mapped)
unknown = sorted(mapped - mc_ids)
approx_ids = sorted(k for k, es in by_mc.items() if all(e.get('approx') for e in es))
partial = sorted(k for k, es in by_mc.items() if any(e.get('approx') for e in es) and k not in approx_ids)
to_air = sorted(k for k, es in by_mc.items() if all(e['vs'] == AIR for e in es))

HEADER = '''# Odasio block mapping: Minecraft Java Edition (1.18 - 1.21.x) -> Vintage Story 1.22.7
# GENERATED by a script (see docs/mapping.md), then meant to be hand-edited.
#
# Schema
# ------
# [settings]
#   rock = "granite"   # value substituted for "{rock}" in any `vs` code (MC stone, cobblestone,
#                      # stone bricks, gravel...). Must be one of `safe_rocks`.
#
# [default]            # used when no [[block]] rule matches
#   solid     = VS code for unknown blocks that are full/solid in MC
#   non_solid = VS code for unknown non-solid blocks (plants, decorations, technical)
#
# [[block]]            # one rule; rules are evaluated in file order, FIRST MATCH WINS
#   mc     = "minecraft:oak_log"         # namespaced MC block id (required)
#   when   = { axis = "y" }              # optional blockstate filter. Every listed property must
#                                        # equal the MC value (strings, as in the chunk palette);
#                                        # an array means "any of". Unlisted properties are ignored.
#   vs     = "game:log-placed-oak-ud"    # full VS block code incl. domain (required)
#   approx = true                        # optional, default false: no faithful VS equivalent
#   fluid  = "game:water-still-7"        # optional: also put this liquid in the VS fluid layer
#                                        # (always-underwater plants). Independently of this,
#                                        # MC `waterlogged = "true"` should yield water in the
#                                        # fluid layer for any block.
#   be     = "GenericTypedContainer"     # optional, informative: the VS block has this block-entity
#                                        # class; the converter must create the BE or the block
#                                        # may be inert/broken in game.
#   note   = "..."                       # optional free text
#
# All `vs` codes were validated against assets/vs_block_codes.txt (expanded from VS 1.22.7 assets).
'''

with open(OUT, 'w') as f:
    f.write(HEADER)
    f.write('\n[settings]\nrock = "%s"\nsafe_rocks = %s\n' % (DEFAULT_ROCK, tval(safe_rocks)))
    f.write('\n[default]\nsolid = "game:rock-{rock}"\nnon_solid = "game:air"\n')
    for mc in sorted(by_mc):
        f.write('\n# ---- %s%s\n' % (mc, '' if mc in mc_ids else '  (not in 1.21.1 jar: newer or legacy id)'))
        for e in by_mc[mc]:
            f.write('[[block]]\n')
            for k in ('mc', 'when', 'vs', 'approx', 'fluid', 'be', 'note'):
                if k in e:
                    v = e[k]
                    f.write('%s = %s\n' % (k, inline(v) if k == 'when' else tval(v)))

stats = {
    'entries': len(entries), 'mc_ids_known_1_21_1': len(mcblocks) - len(EXTRA_MC),
    'mc_ids_total_considered': len(mc_ids), 'mc_ids_mapped': len(mapped & mc_ids),
    'mc_ids_unmapped': unmapped, 'mapped_ids_not_in_list': unknown,
    'fully_approx_ids': len(approx_ids), 'partially_approx_ids': len(partial),
    'exact_ids': len(mapped) - len(approx_ids) - len(partial),
    'removed_to_air_ids': len(to_air), 'approx_entries': sum(1 for e in entries if e.get('approx')),
    'distinct_vs_codes': len(set(e['vs'] for e in entries)), 'safe_rocks': safe_rocks,
    'entries_with_be': sum(1 for e in entries if 'be' in e), 'errors': errors,
}
json.dump(stats, open(STATS, 'w'), indent=1)
print(json.dumps({k: (v if not isinstance(v, list) or len(v) < 40 else '%d items' % len(v)) for k, v in stats.items()}, indent=1))
