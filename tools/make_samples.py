#!/usr/bin/env python3
"""Writes one small test build (a hut: floor, walls, windows, a door, a roof of
stairs, a torch and a chest) in every format the mod reads, into a folder
(default: the game's `schematics/samples/`). Its NBT writer is its own, made
from the formats' specs, so the mod's readers are checked against something
other than themselves.
"""
import gzip
import struct
import sys
from pathlib import Path

OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / '.local/share/horizon/schematics/samples'

# --- NBT -------------------------------------------------------------------
class Byte(int): pass
class Short(int): pass
class Int(int): pass
class Long(int): pass
class Double(float): pass
class ByteArray(bytes): pass
class IntArray(list): pass
class LongArray(list): pass
class List(list): pass

def tag_id(v):
    for t, i in ((Byte, 1), (Short, 2), (Int, 3), (Long, 4), (Double, 6), (ByteArray, 7), (str, 8),
                 (List, 9), (dict, 10), (IntArray, 11), (LongArray, 12)):
        if isinstance(v, t):
            return i
    raise TypeError(type(v))

def payload(v, e):
    i = tag_id(v)
    if i == 1: return struct.pack(e + 'b', v)
    if i == 2: return struct.pack(e + 'h', v)
    if i == 3: return struct.pack(e + 'i', v)
    if i == 4: return struct.pack(e + 'q', v)
    if i == 6: return struct.pack(e + 'd', v)
    if i == 7: return struct.pack(e + 'i', len(v)) + bytes(v)
    if i == 8:
        b = v.encode()
        return struct.pack(e + 'H', len(b)) + b
    if i == 9:
        first = tag_id(v[0]) if v else 0
        return bytes([first]) + struct.pack(e + 'i', len(v)) + b''.join(payload(x, e) for x in v)
    if i == 10:
        out = b''
        for k, x in v.items():
            out += bytes([tag_id(x)]) + payload(k, e) + payload(x, e)
        return out + b'\0'
    if i == 11: return struct.pack(e + 'i', len(v)) + b''.join(struct.pack(e + 'i', x) for x in v)
    if i == 12: return struct.pack(e + 'i', len(v)) + b''.join(struct.pack(e + 'q', x) for x in v)

def nbt(root, name='', e='>'):
    return bytes([10]) + payload(name, e) + payload(root, e)

# --- The build -------------------------------------------------------------
W, H, L = 5, 5, 5

def hut():
    """{(x, y, z): (name, props)} of the hut."""
    b = {}
    for x in range(W):
        for z in range(L):
            b[(x, 0, z)] = ('minecraft:cobblestone', {})
    for y in (1, 2):
        for x in range(W):
            for z in range(L):
                if x in (0, W - 1) or z in (0, L - 1):
                    b[(x, y, z)] = ('minecraft:oak_planks', {})
    b[(0, 2, 2)] = ('minecraft:glass', {})
    b[(4, 2, 2)] = ('minecraft:glass', {})
    b[(2, 1, 0)] = ('minecraft:oak_door', {'facing': 'south', 'half': 'lower', 'hinge': 'left', 'open': 'false', 'powered': 'false'})
    b[(2, 2, 0)] = ('minecraft:oak_door', {'facing': 'south', 'half': 'upper', 'hinge': 'left', 'open': 'false', 'powered': 'false'})
    for x in range(W):
        for z in range(L):
            facing = 'north' if z < 2 else 'south' if z > 2 else None
            b[(x, 3, z)] = ('minecraft:oak_stairs', {'facing': facing, 'half': 'bottom', 'shape': 'straight', 'waterlogged': 'false'}) if facing else ('minecraft:oak_slab', {'type': 'bottom', 'waterlogged': 'false'})
    b[(1, 1, 3)] = ('minecraft:chest', {'facing': 'south', 'type': 'single', 'waterlogged': 'false'})
    b[(3, 1, 3)] = ('minecraft:wall_torch', {'facing': 'north'})
    b[(3, 1, 1)] = ('minecraft:red_wool', {})
    return b

CHEST = {'Items': List([{'Slot': Byte(0), 'id': 'minecraft:diamond', 'count': Int(3)}])}

def state_str(name, props):
    return name + ('[' + ','.join(f'{k}={v}' for k, v in sorted(props.items())) + ']' if props else '')

def palette_of(b):
    pal = [('minecraft:air', {})]
    for s in b.values():
        if s not in pal:
            pal.append(s)
    return pal

def index(x, y, z):
    return (y * L + z) * W + x

def blocks_array(b, pal):
    arr = [0] * (W * H * L)
    for (x, y, z), s in b.items():
        arr[index(x, y, z)] = pal.index(s)
    return arr

def to_i64(u):
    return u - (1 << 64) if u >= 1 << 63 else u

# --- .litematic ------------------------------------------------------------
def litematic(b):
    pal = palette_of(b)
    bits = max(2, (len(pal) - 1).bit_length())
    vals = blocks_array(b, pal)
    words = [0] * ((len(vals) * bits + 63) // 64)
    for i, v in enumerate(vals):
        start = i * bits
        w, off = start // 64, start % 64
        words[w] |= (v << off) & ((1 << 64) - 1)
        if off + bits > 64:
            words[w + 1] |= v >> (64 - off)
    vec = lambda x, y, z: {'x': Int(x), 'y': Int(y), 'z': Int(z)}
    region = {
        'Position': vec(0, 0, 0),
        'Size': vec(W, H, L),
        'BlockStatePalette': List([{'Name': n, 'Properties': {k: v for k, v in p.items()}} if p else {'Name': n} for n, p in pal]),
        'BlockStates': LongArray([to_i64(w) for w in words]),
        'TileEntities': List([dict({'x': Int(1), 'y': Int(1), 'z': Int(3), 'id': 'minecraft:chest'}, **CHEST)]),
        'Entities': List([]),
        'PendingBlockTicks': List([]),
        'PendingFluidTicks': List([]),
    }
    root = {
        'MinecraftDataVersion': Int(4671), 'Version': Int(6), 'SubVersion': Int(1),
        'Metadata': {'Name': 'Sample hut (litematic)', 'Author': 'samples', 'Description': '',
                     'RegionCount': Int(1), 'TotalBlocks': Int(len(b)), 'TotalVolume': Int(W * H * L),
                     'EnclosingSize': vec(W, H, L), 'TimeCreated': Long(0), 'TimeModified': Long(0)},
        'Regions': {'Hut': region},
    }
    return gzip.compress(nbt(root))

# --- Sponge ----------------------------------------------------------------
def varints(vals):
    out = bytearray()
    for v in vals:
        while True:
            byte = v & 0x7F
            v >>= 7
            if v:
                out.append(byte | 0x80)
            else:
                out.append(byte)
                break
    return ByteArray(bytes(out))

def sponge(b, version):
    pal = palette_of(b)
    palette = {state_str(n, p): Int(i) for i, (n, p) in enumerate(pal)}
    data = varints(blocks_array(b, pal))
    be = {'Pos': IntArray([1, 1, 3]), 'Id': 'minecraft:chest'}
    common = {'Version': Int(version), 'DataVersion': Int(4671), 'Width': Short(W), 'Height': Short(H),
              'Length': Short(L), 'Offset': IntArray([0, 0, 0])}
    if version == 2:
        root = dict(common, PaletteMax=Int(len(pal)), Palette=palette, BlockData=data,
                    BlockEntities=List([dict(be, **CHEST)]))
        return gzip.compress(nbt(root, 'Schematic'))
    root = {'Schematic': dict(common, Blocks={'Palette': palette, 'Data': data,
                                              'BlockEntities': List([dict(be, Data=CHEST)])})}
    return gzip.compress(nbt(root))

# --- MCEdit (numeric ids) ----------------------------------------------------
LEGACY = {
    'minecraft:cobblestone': (4, lambda p: 0),
    'minecraft:oak_planks': (5, lambda p: 0),
    'minecraft:glass': (20, lambda p: 0),
    'minecraft:red_wool': (35, lambda p: 14),
    'minecraft:oak_door': (64, lambda p: (8 if p['half'] == 'upper' else 1)),
    'minecraft:oak_stairs': (53, lambda p: {'east': 0, 'west': 1, 'south': 2, 'north': 3}[p['facing']]),
    'minecraft:oak_slab': (126, lambda p: 0),
    'minecraft:chest': (54, lambda p: 3),
    'minecraft:wall_torch': (50, lambda p: 4),
}

def mcedit(b):
    ids = bytearray(W * H * L)
    meta = bytearray(W * H * L)
    for (x, y, z), (n, p) in b.items():
        i, m = LEGACY[n]
        ids[index(x, y, z)] = i
        meta[index(x, y, z)] = m(p)
    root = {'Width': Short(W), 'Height': Short(H), 'Length': Short(L), 'Materials': 'Alpha',
            'Blocks': ByteArray(bytes(ids)), 'Data': ByteArray(bytes(meta)),
            'TileEntities': List([dict({'x': Int(1), 'y': Int(1), 'z': Int(3), 'id': 'Chest'}, Items=List([]))]),
            'Entities': List([])}
    return gzip.compress(nbt(root, 'Schematic'))

# --- Structure file --------------------------------------------------------
def structure(b):
    pal = palette_of(b)[1:]
    blocks = []
    for (x, y, z), s in sorted(b.items()):
        entry = {'pos': List([Int(x), Int(y), Int(z)]), 'state': Int(pal.index(s))}
        if s[0] == 'minecraft:chest':
            entry['nbt'] = dict({'id': 'minecraft:chest'}, **CHEST)
        blocks.append(entry)
    root = {'DataVersion': Int(4671), 'size': List([Int(W), Int(H), Int(L)]),
            'palette': List([{'Name': n, 'Properties': dict(p)} if p else {'Name': n} for n, p in pal]),
            'blocks': List(blocks), 'entities': List([])}
    return gzip.compress(nbt(root))

# --- Bedrock ---------------------------------------------------------------
def bedrock(b):
    def translate(n, p):
        if n == 'minecraft:oak_stairs':
            return n, {'weirdo_direction': Int({'east': 0, 'west': 1, 'south': 2, 'north': 3}[p['facing']]), 'upside_down_bit': Byte(0)}
        if n == 'minecraft:oak_door':
            return 'minecraft:wooden_door', {'direction': Int(1), 'upper_block_bit': Byte(p['half'] == 'upper'), 'door_hinge_bit': Byte(0), 'open_bit': Byte(0)}
        if n == 'minecraft:oak_slab':
            return n, {'minecraft:vertical_half': 'bottom'}
        if n == 'minecraft:chest':
            return n, {'minecraft:cardinal_direction': 'south'}
        if n == 'minecraft:wall_torch':
            return 'minecraft:torch', {'torch_facing_direction': 'north'}
        if n == 'minecraft:red_wool':
            return 'minecraft:wool', {'color': 'red'}
        return n, {}
    pal = palette_of(b)
    idx = []
    for x in range(W):
        for y in range(H):
            for z in range(L):
                s = b.get((x, y, z))
                idx.append(Int(pal.index(s) if s else 0))
    palette = []
    for n, p in pal:
        name, states = translate(n, p)
        palette.append({'name': name, 'states': states, 'version': Int(17959425)})
    chest = (1 * H + 1) * L + 3
    root = {'format_version': Int(1), 'size': List([Int(W), Int(H), Int(L)]),
            'structure': {'block_indices': List([List(idx), List([Int(-1)] * len(idx))]),
                          'entities': List([]),
                          'palette': {'default': {'block_palette': List(palette),
                                                  'block_position_data': {str(chest): {'block_entity_data': {'id': 'Chest', 'Items': List([])}}}}}},
            'structure_world_origin': List([Int(0), Int(0), Int(0)])}
    return nbt(root, '', '<')

# --- Axiom -----------------------------------------------------------------
def axiom(b):
    pal = palette_of(b)
    bits = max(4, (len(pal) - 1).bit_length())
    per = 64 // bits
    words = [0] * ((4096 + per - 1) // per)
    for (x, y, z), s in b.items():
        i = (y << 8) | (z << 4) | x
        words[i // per] |= pal.index(s) << ((i % per) * bits)
    body = {'BlockRegion': List([{'X': Int(0), 'Y': Int(0), 'Z': Int(0), 'BlockStates': {
        'palette': List([{'Name': n, 'Properties': dict(p)} if p else {'Name': n} for n, p in pal]),
        'data': LongArray([to_i64(w) for w in words])}}]),
            'BlockEntities': List([dict({'x': Int(1), 'y': Int(1), 'z': Int(3), 'id': 'minecraft:chest'}, **CHEST)])}
    header = nbt({'Name': 'Sample hut (axiom)', 'Author': 'samples', 'Tags': List([]), 'BlockCount': Int(len(b))})
    data = gzip.compress(nbt(body))
    thumb = b'\x89PNG\r\n\x1a\n'
    out = struct.pack('>I', 0x0AE5BB36)
    for part in (header, thumb, data):
        out += struct.pack('>I', len(part)) + part
    return out

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    b = hut()
    files = {
        'hut.litematic': litematic(b),
        'hut_v2.schem': sponge(b, 2),
        'hut_v3.schem': sponge(b, 3),
        'hut_legacy.schematic': mcedit(b),
        'hut.nbt': structure(b),
        'hut.mcstructure': bedrock(b),
        'hut.bp': axiom(b),
    }
    for name, data in files.items():
        (OUT / name).write_bytes(data)
        print(OUT / name, len(data))

if __name__ == '__main__':
    main()
