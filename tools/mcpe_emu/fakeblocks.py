"""给 Block::m* 指针装上只含 id 的假方块（MCPE 0.14 标准 id），供 biome 构造读取。"""
IDS = dict(Air=0, Stone=1, Grass=2, Dirt=3, Cobblestone=4, WoodPlanks=5, Sapling=6, Bedrock=7,
    FlowingWater=8, StillWater=9, FlowingLava=10, StillLava=11, Sand=12, Gravel=13, GoldOre=14,
    IronOre=15, CoalOre=16, Log=17, Leaves=18, Sponge=19, Glass=20, LapisOre=21, LapisBlock=22,
    SandStone=24, Wool=35, YellowFlower=37, RedFlower=38, BrownMushroom=39, RedMushroom=40,
    Obsidian=49, DiamondOre=56, RedStoneOre=73, TopSnow=78, Ice=79, Snow=80, Cactus=81, Clay=82,
    Reeds=83, Pumpkin=86, Netherrack=87, SoulSand=88, GlowStone=89, Mycelium=110, Waterlily=111,
    EmeraldOre=129, StainedClay=159, Leaves2=161, Log2=162, HardenedClay=172, PackedIce=174,
    DoublePlant=175, RedSandstone=179, GrassPathBlock=198, Podzol=243, Tallgrass=31, DeadBush=32,
    Vine=106, MonsterStoneEgg=97, EndStone=121)

def install(e):
    import struct
    for k, a in e.syms.items():
        if not k.startswith('_ZN5Block') or not k.endswith('E'):
            continue
        # _ZN5Block<len><name>E
        rest = k[len('_ZN5Block'):-1]
        n = ''
        while rest and rest[0].isdigit():
            n += rest[0]; rest = rest[1:]
        if not n or len(rest) != int(n) or not rest.startswith('m'):
            continue
        name = rest[1:]
        if name in ('Blocks', 'OwnedBlocks', 'BlockLookupMap', 'TerrainTextureAtlas', 'Solid',
                    'Translucency', 'LightBlock', 'LightEmission', 'ShouldTick'):
            continue
        blk = e.alloc(0x100)
        e.uc.mem_write(blk + 4, bytes([IDS.get(name, 255)]))
        e.w32(a, blk)
