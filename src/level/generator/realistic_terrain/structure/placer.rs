//! 村庄组件放置。所有内部方块判断使用 Java state，区块边界处转换为 native state。

use std::sync::Arc;

use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::native_block_mapper::{to_java, to_native};
use crate::level::generator::world::GenWorld;

use super::placement::{BoundingBox, population_clip};
use super::topology::{Direction, PieceKind, VillagePiece, VillageStructure};

pub trait TerrainHeightProvider {
    fn terrain_height(&mut self, x: i32, z: i32) -> i32;
}

pub fn generate<T: TerrainHeightProvider>(
    world: &mut GenWorld,
    terrain: &mut T,
    random: &mut JavaRandom,
    starts: &[Arc<VillageStructure>],
    chunk_x: i32,
    chunk_z: i32,
) -> bool {
    let mut placer = Placer {
        world,
        terrain,
        random,
        clip: population_clip(chunk_x, chunk_z),
    };
    let mut placed = false;
    for start in starts {
        placed = true;
        // 缓存只保存拓扑。HPos、箱体位移和 chest 标志都属于本次放置。
        let mut pieces = start.pieces.clone();
        for piece in &mut pieces {
            if piece.bounding_box.intersects(&placer.clip) {
                placer.place_piece(piece);
            }
        }
    }
    placed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pos {
    x: i32,
    y: i32,
    z: i32,
}

impl Pos {
    fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
    fn down(self) -> Self {
        Self::new(self.x, self.y - 1, self.z)
    }
    fn up(self) -> Self {
        Self::new(self.x, self.y + 1, self.z)
    }
    fn add(self, x: i32, y: i32, z: i32) -> Self {
        Self::new(self.x + x, self.y + y, self.z + z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    North,
    East,
    South,
    West,
    Up,
}

struct Placer<'w, 'g, 't, 'r, T: TerrainHeightProvider> {
    world: &'w mut GenWorld<'g>,
    terrain: &'t mut T,
    random: &'r mut JavaRandom,
    clip: BoundingBox,
}

impl<T: TerrainHeightProvider> Placer<'_, '_, '_, '_, T> {
    fn place_piece(&mut self, piece: &mut VillagePiece) {
        match piece.kind {
            PieceKind::Start => self.well(piece),
            PieceKind::Path => self.path(piece),
            PieceKind::Torch => self.lamp(piece),
            PieceKind::Field1 => self.field1(piece),
            PieceKind::Field2 => self.field2(piece),
            PieceKind::Church => self.church(piece),
            PieceKind::Hall => self.hall(piece),
            PieceKind::House1 => self.house1(piece),
            PieceKind::House2 => self.house2(piece),
            PieceKind::House3 => self.house3(piece),
            PieceKind::House4Garden => self.house4(piece),
            PieceKind::WoodHut => self.wood_hut(piece),
        }
    }

    fn prepare(&mut self, piece: &mut VillagePiece, height: i32, well: bool) -> bool {
        if piece.average_ground >= 0 {
            return true;
        }
        let mut sum = 0_i64;
        let mut count = 0_i64;
        for z in piece.bounding_box.min_z..=piece.bounding_box.max_z {
            for x in piece.bounding_box.min_x..=piece.bounding_box.max_x {
                // 始终读取完整原始 footprint，不裁剪，也不扫描已放置的屋顶。
                sum += self.terrain.terrain_height(x, z).max(62) as i64;
                count += 1;
            }
        }
        if count == 0 {
            return false;
        }
        piece.average_ground = (sum / count) as i32;
        let offset =
            piece.average_ground - piece.bounding_box.max_y + if well { 3 } else { height - 1 };
        piece.bounding_box.offset(0, offset, 0);
        true
    }

    fn inside(&self, q: Pos) -> bool {
        q.x >= self.clip.min_x
            && q.x <= self.clip.max_x
            && q.y >= self.clip.min_y
            && q.y <= self.clip.max_y
            && q.z >= self.clip.min_z
            && q.z <= self.clip.max_z
    }

    fn pos(&self, piece: &VillagePiece, x: i32, y: i32, z: i32) -> Pos {
        let b = piece.bounding_box;
        let wx = match piece.direction {
            Direction::North | Direction::South => b.min_x + x,
            Direction::West => b.max_x - z,
            Direction::East => b.min_x + z,
        };
        let wz = match piece.direction {
            Direction::North => b.max_z - z,
            Direction::South => b.min_z + z,
            Direction::West | Direction::East => b.min_z + x,
        };
        Pos::new(wx, b.min_y + y, wz)
    }

    fn world_state(&self, q: Pos) -> i32 {
        to_java(self.world.block_state(q.x, q.y, q.z))
    }

    fn set_world(&mut self, q: Pos, state: i32) {
        self.world.set_block_state(q.x, q.y, q.z, to_native(state));
    }

    fn set(&mut self, piece: &VillagePiece, state: i32, x: i32, y: i32, z: i32) {
        let q = self.pos(piece, x, y, z);
        if self.inside(q) {
            let state = self.rotate(piece, state);
            self.set_world(q, state);
        }
    }

    fn get(&self, piece: &VillagePiece, x: i32, y: i32, z: i32) -> i32 {
        let q = self.pos(piece, x, y, z);
        if self.inside(q) {
            self.world_state(q)
        } else {
            0
        }
    }

    fn fill(
        &mut self,
        piece: &VillagePiece,
        x1: i32,
        y1: i32,
        z1: i32,
        x2: i32,
        y2: i32,
        z2: i32,
        edge: i32,
    ) {
        self.fill_ex(piece, x1, y1, z1, x2, y2, z2, edge, edge, false);
    }

    fn fill_inside(
        &mut self,
        piece: &VillagePiece,
        x1: i32,
        y1: i32,
        z1: i32,
        x2: i32,
        y2: i32,
        z2: i32,
        edge: i32,
        inside: i32,
    ) {
        self.fill_ex(piece, x1, y1, z1, x2, y2, z2, edge, inside, false);
    }

    fn fill_ex(
        &mut self,
        piece: &VillagePiece,
        x1: i32,
        y1: i32,
        z1: i32,
        x2: i32,
        y2: i32,
        z2: i32,
        edge: i32,
        inside: i32,
        existing: bool,
    ) {
        for y in y1..=y2 {
            for x in x1..=x2 {
                for z in z1..=z2 {
                    if existing && self.get(piece, x, y, z) >> 4 == 0 {
                        continue;
                    }
                    let state = if y != y1 && y != y2 && x != x1 && x != x2 && z != z1 && z != z2 {
                        inside
                    } else {
                        edge
                    };
                    self.set(piece, state, x, y, z);
                }
            }
        }
    }

    fn clear_up(&mut self, piece: &VillagePiece, x: i32, y: i32, z: i32) {
        let mut q = self.pos(piece, x, y, z);
        if !self.inside(q) {
            return;
        }
        while !self.world.is_air(q.x, q.y, q.z) && q.y < 255 {
            self.set_world(q, 0);
            q = q.up();
        }
    }

    fn down(&mut self, piece: &VillagePiece, state: i32, x: i32, y: i32, z: i32) {
        let mut q = self.pos(piece, x, y, z);
        if !self.inside(q) {
            return;
        }
        let state = self.rotate(piece, state);
        while (self.world.is_air(q.x, q.y, q.z) || block_rules::is_liquid(self.world_state(q)))
            && q.y > 1
        {
            self.set_world(q, state);
            q = q.down();
        }
    }

    fn style(&self, piece: &VillagePiece, state: i32) -> i32 {
        let id = state >> 4;
        let meta = state & 15;
        match piece.structure_type {
            1 => {
                if matches!(id, 17 | 162 | 4) {
                    24 << 4
                } else if id == 5 {
                    (24 << 4) | 2
                } else if matches!(id, 53 | 67) {
                    (128 << 4) | meta
                } else if id == 13 {
                    24 << 4
                } else {
                    state
                }
            }
            3 => {
                if matches!(id, 17 | 162) {
                    (17 << 4) | 1 | (meta & 12)
                } else if id == 5 {
                    (5 << 4) | 1
                } else if id == 53 {
                    (134 << 4) | meta
                } else if id == 85 {
                    188 << 4
                } else {
                    state
                }
            }
            2 => {
                if matches!(id, 17 | 162) {
                    (162 << 4) | (meta & 12)
                } else if id == 5 {
                    (5 << 4) | 4
                } else if id == 53 {
                    (163 << 4) | meta
                } else if id == 4 {
                    162 << 4
                } else if id == 85 {
                    192 << 4
                } else {
                    state
                }
            }
            _ => state,
        }
    }

    fn face_meta(id: i32, face: Face) -> i32 {
        if matches!(id, 53 | 67 | 109 | 128 | 134 | 163) {
            match face {
                Face::East => 0,
                Face::West => 1,
                Face::South => 2,
                Face::North => 3,
                Face::Up => 0,
            }
        } else if id == 50 {
            match face {
                Face::East => 1,
                Face::West => 2,
                Face::South => 3,
                Face::North => 4,
                Face::Up => 5,
            }
        } else if matches!(id, 54 | 61 | 65 | 68) {
            match face {
                Face::North => 2,
                Face::South => 3,
                Face::West => 4,
                Face::East => 5,
                Face::Up => 0,
            }
        } else if matches!(id, 64 | 193 | 196) {
            match face {
                Face::East => 0,
                Face::South => 1,
                Face::West => 2,
                Face::North => 3,
                Face::Up => 0,
            }
        } else {
            0
        }
    }

    fn faced(id: i32, face: Face) -> i32 {
        Self::faced_extra(id, face, 0)
    }

    fn faced_extra(id: i32, face: Face, extra: i32) -> i32 {
        (id << 4) | Self::face_meta(id, face) | extra
    }

    fn transform_face(direction: Direction, face: Face) -> Face {
        if face == Face::Up || direction == Direction::North {
            return face;
        }
        if direction == Direction::South {
            return match face {
                Face::North => Face::South,
                Face::South => Face::North,
                _ => face,
            };
        }
        let face = if direction == Direction::West {
            match face {
                Face::North => Face::South,
                Face::South => Face::North,
                _ => face,
            }
        } else {
            face
        };
        match face {
            Face::North => Face::East,
            Face::East => Face::South,
            Face::South => Face::West,
            Face::West => Face::North,
            Face::Up => Face::Up,
        }
    }

    fn rotate(&self, piece: &VillagePiece, state: i32) -> i32 {
        let id = state >> 4;
        let meta = state & 15;
        let mut face = None;
        let mut extra = 0;
        if matches!(id, 53 | 67 | 109 | 128 | 134 | 163) {
            face = Some([Face::East, Face::West, Face::South, Face::North][(meta & 3) as usize]);
            extra = meta & 12;
        } else if id == 50 && (1..=4).contains(&meta) {
            face = Some(match meta {
                1 => Face::East,
                2 => Face::West,
                3 => Face::South,
                _ => Face::North,
            });
        } else if matches!(id, 54 | 61 | 65 | 68) && (2..=5).contains(&meta) {
            face = Some(match meta {
                2 => Face::North,
                3 => Face::South,
                4 => Face::West,
                _ => Face::East,
            });
        } else if matches!(id, 64 | 193 | 196) && (meta & 8) == 0 {
            face = Some([Face::East, Face::South, Face::West, Face::North][(meta & 3) as usize]);
            extra = meta & 12;
        }
        match face {
            Some(face) => Self::faced_extra(id, Self::transform_face(piece.direction, face), extra),
            None => state,
        }
    }

    fn stair(&self, piece: &VillagePiece, id: i32, face: Face) -> i32 {
        self.style(piece, Self::faced(id, face))
    }

    fn torch(&mut self, piece: &VillagePiece, face: Face, x: i32, y: i32, z: i32) {
        if !piece.zombie {
            self.set(piece, Self::faced(50, face), x, y, z);
        }
    }

    fn door(&mut self, piece: &VillagePiece, x: i32, y: i32, z: i32) {
        if piece.zombie {
            return;
        }
        let id = if piece.structure_type == 2 {
            196
        } else if piece.structure_type == 3 {
            193
        } else {
            64
        };
        self.set(piece, Self::faced(id, Face::North), x, y, z);
        let upper = 8 + i32::from(matches!(
            piece.direction,
            Direction::South | Direction::West
        ));
        self.set(piece, (id << 4) | upper, x, y + 1, z);
    }

    fn step(&mut self, piece: &VillagePiece, state: i32, x: i32) {
        if self.get(piece, x, 0, -1) >> 4 == 0 && self.get(piece, x, -1, -1) >> 4 != 0 {
            self.set(piece, state, x, 0, -1);
            if self.get(piece, x, -1, -1) >> 4 == 208 {
                self.set(piece, 2 << 4, x, -1, -1);
            }
        }
    }

    fn foundations(
        &mut self,
        piece: &VillagePiece,
        width: i32,
        depth: i32,
        height: i32,
        state: i32,
    ) {
        for z in 0..depth {
            for x in 0..width {
                self.clear_up(piece, x, height, z);
                self.down(piece, state, x, -1, z);
            }
        }
    }

    fn crop(roll: i32) -> (i32, i32) {
        if roll <= 1 {
            (141, 7)
        } else if roll <= 3 {
            (142, 7)
        } else if roll == 4 {
            (244, 3)
        } else {
            (59, 7)
        }
    }

    fn crop_state(&mut self, crop: (i32, i32)) -> i32 {
        let min = crop.1 / 3;
        (crop.0 << 4) | (min + self.random.next_int_bound(crop.1 - min + 1))
    }

    fn offset_face(q: Pos, face: Face) -> Pos {
        match face {
            Face::North => q.add(0, 0, -1),
            Face::South => q.add(0, 0, 1),
            Face::West => q.add(-1, 0, 0),
            Face::East => q.add(1, 0, 0),
            Face::Up => q.up(),
        }
    }

    fn opposite(face: Face) -> Face {
        match face {
            Face::North => Face::South,
            Face::South => Face::North,
            Face::West => Face::East,
            Face::East => Face::West,
            Face::Up => Face::Up,
        }
    }

    fn chest_state(&self, piece: &VillagePiece, q: Pos) -> i32 {
        let state = self.rotate(piece, Self::faced(54, Face::North));
        let mut found = None;
        let mut multiple = false;
        for face in [Face::South, Face::West, Face::North, Face::East] {
            let neighbor = self.world_state(Self::offset_face(q, face));
            if neighbor >> 4 == 54 {
                return state;
            }
            if block_rules::is_full_block(neighbor) {
                if found.is_some() {
                    multiple = true;
                    break;
                }
                found = Some(face);
            }
        }
        if !multiple && found.is_some() {
            return Self::faced(54, Self::opposite(found.unwrap()));
        }
        let mut face = match state & 15 {
            2 => Face::North,
            3 => Face::South,
            4 => Face::West,
            _ => Face::East,
        };
        if block_rules::is_full_block(self.world_state(Self::offset_face(q, face))) {
            face = Self::opposite(face);
        }
        if block_rules::is_full_block(self.world_state(Self::offset_face(q, face))) {
            face = match face {
                Face::North => Face::East,
                Face::East => Face::South,
                Face::South => Face::West,
                Face::West => Face::North,
                Face::Up => Face::Up,
            };
        }
        if block_rules::is_full_block(self.world_state(Self::offset_face(q, face))) {
            face = Self::opposite(face);
        }
        Self::faced(54, face)
    }

    fn top_solid_or_liquid(&self, x: i32, z: i32) -> i32 {
        for y in (0..256).rev() {
            let state = self.world_state(Pos::new(x, y, z));
            if block_rules::is_solid(state) || block_rules::is_liquid(state) {
                return y + 1;
            }
        }
        0
    }

    fn path(&mut self, piece: &VillagePiece) {
        let path = self.style(piece, 208 << 4);
        let planks = self.style(piece, 5 << 4);
        let gravel = self.style(piece, 13 << 4);
        let cobble = self.style(piece, 4 << 4);
        for x in piece.bounding_box.min_x..=piece.bounding_box.max_x {
            for z in piece.bounding_box.min_z..=piece.bounding_box.max_z {
                if !self.inside(Pos::new(x, 64, z)) {
                    continue;
                }
                let mut q = Pos::new(x, self.top_solid_or_liquid(x, z) - 1, z);
                if q.y < 63 {
                    q = Pos::new(x, 62, z);
                }
                while q.y >= 62 {
                    let state = self.world_state(q);
                    let id = state >> 4;
                    if id == 2 && self.world.is_air(q.x, q.y + 1, q.z) {
                        self.set_world(q, path);
                        break;
                    }
                    if matches!(id, 8 | 9 | 10 | 11) {
                        self.set_world(q, planks);
                        break;
                    }
                    if matches!(id, 12 | 24 | 179) {
                        self.set_world(q, gravel);
                        self.set_world(q.down(), cobble);
                        break;
                    }
                    q = q.down();
                }
            }
        }
    }

    fn well(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 16, true) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let f = self.style(piece, 85 << 4);
        self.fill_inside(piece, 1, 0, 1, 4, 12, 4, c, 8 << 4);
        for [x, y, z] in [[2, 12, 2], [3, 12, 2], [2, 12, 3], [3, 12, 3]] {
            self.set(piece, 0, x, y, z);
        }
        for [x, y, z] in [
            [1, 13, 1],
            [1, 14, 1],
            [4, 13, 1],
            [4, 14, 1],
            [1, 13, 4],
            [1, 14, 4],
            [4, 13, 4],
            [4, 14, 4],
        ] {
            self.set(piece, f, x, y, z);
        }
        self.fill(piece, 1, 15, 1, 4, 15, 4, c);
        for z in 0..=5 {
            for x in 0..=5 {
                if x == 0 || x == 5 || z == 0 || z == 5 {
                    self.set(piece, c, x, 11, z);
                    self.clear_up(piece, x, 12, z);
                }
            }
        }
    }

    fn lamp(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 4, false) {
            return;
        }
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 0, 0, 0, 2, 3, 1, 0);
        for y in 0..=2 {
            self.set(piece, f, 1, y, 0);
        }
        self.set(piece, 35 << 4, 1, 3, 0);
        self.torch(piece, Face::East, 2, 3, 0);
        self.torch(piece, Face::North, 1, 3, 1);
        self.torch(piece, Face::West, 0, 3, 0);
        self.torch(piece, Face::South, 1, 3, -1);
    }

    fn field1(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 4, false) {
            return;
        }
        let log = self.style(piece, 17 << 4);
        self.fill(piece, 0, 1, 0, 12, 4, 8, 0);
        for [a, b] in [[1, 2], [4, 5], [7, 8], [10, 11]] {
            self.fill(piece, a, 0, 1, b, 0, 7, 60 << 4);
        }
        for x in [0, 6, 12] {
            self.fill(piece, x, 0, 0, x, 0, 8, log);
        }
        self.fill(piece, 1, 0, 0, 11, 0, 0, log);
        self.fill(piece, 1, 0, 8, 11, 0, 8, log);
        self.fill(piece, 3, 0, 1, 3, 0, 7, 9 << 4);
        self.fill(piece, 9, 0, 1, 9, 0, 7, 9 << 4);
        let crops: Vec<_> = piece.crops.iter().map(|&roll| Self::crop(roll)).collect();
        for z in 1..=7 {
            for [a, b, index] in [[1, 2, 0], [4, 5, 1], [7, 8, 2], [10, 11, 3]] {
                let first = self.crop_state(crops[index as usize]);
                self.set(piece, first, a, 1, z);
                let second = self.crop_state(crops[index as usize]);
                self.set(piece, second, b, 1, z);
            }
        }
        self.foundations(piece, 13, 9, 4, 3 << 4);
    }

    fn field2(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 4, false) {
            return;
        }
        let log = self.style(piece, 17 << 4);
        self.fill(piece, 0, 1, 0, 6, 4, 8, 0);
        self.fill(piece, 1, 0, 1, 2, 0, 7, 60 << 4);
        self.fill(piece, 4, 0, 1, 5, 0, 7, 60 << 4);
        self.fill(piece, 0, 0, 0, 0, 0, 8, log);
        self.fill(piece, 6, 0, 0, 6, 0, 8, log);
        self.fill(piece, 1, 0, 0, 5, 0, 0, log);
        self.fill(piece, 1, 0, 8, 5, 0, 8, log);
        self.fill(piece, 3, 0, 1, 3, 0, 7, 9 << 4);
        let a = Self::crop(piece.crops[0]);
        let b = Self::crop(piece.crops[1]);
        for z in 1..=7 {
            let state = self.crop_state(a);
            self.set(piece, state, 1, 1, z);
            let state = self.crop_state(a);
            self.set(piece, state, 2, 1, z);
            let state = self.crop_state(b);
            self.set(piece, state, 4, 1, z);
            let state = self.crop_state(b);
            self.set(piece, state, 5, 1, z);
        }
        self.foundations(piece, 7, 9, 4, 3 << 4);
    }

    fn church(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 12, false) {
            return;
        }
        let c = 4 << 4;
        let sn = self.stair(piece, 67, Face::North);
        let sw = self.stair(piece, 67, Face::West);
        let se = self.stair(piece, 67, Face::East);
        self.fill(piece, 1, 1, 1, 3, 3, 7, 0);
        self.fill(piece, 1, 5, 1, 3, 9, 3, 0);
        self.fill(piece, 1, 0, 0, 3, 0, 8, c);
        self.fill(piece, 1, 1, 0, 3, 10, 0, c);
        self.fill(piece, 0, 1, 1, 0, 10, 3, c);
        self.fill(piece, 4, 1, 1, 4, 10, 3, c);
        self.fill(piece, 0, 0, 4, 0, 4, 7, c);
        self.fill(piece, 4, 0, 4, 4, 4, 7, c);
        self.fill(piece, 1, 1, 8, 3, 4, 8, c);
        self.fill(piece, 1, 5, 4, 3, 10, 4, c);
        self.fill(piece, 1, 5, 5, 3, 5, 7, c);
        self.fill(piece, 0, 9, 0, 4, 9, 4, c);
        self.fill(piece, 0, 4, 0, 4, 4, 4, c);
        for [x, y, z] in [
            [0, 11, 2],
            [4, 11, 2],
            [2, 11, 0],
            [2, 11, 4],
            [1, 1, 6],
            [1, 1, 7],
            [2, 1, 7],
            [3, 1, 6],
            [3, 1, 7],
        ] {
            self.set(piece, c, x, y, z);
        }
        for [x, y, z] in [[1, 1, 5], [2, 1, 6], [3, 1, 5]] {
            self.set(piece, sn, x, y, z);
        }
        self.set(piece, sw, 1, 2, 7);
        self.set(piece, se, 3, 2, 7);
        for [x, y, z] in [
            [0, 2, 2],
            [0, 3, 2],
            [4, 2, 2],
            [4, 3, 2],
            [0, 6, 2],
            [0, 7, 2],
            [4, 6, 2],
            [4, 7, 2],
            [2, 6, 0],
            [2, 7, 0],
            [2, 6, 4],
            [2, 7, 4],
            [0, 3, 6],
            [4, 3, 6],
            [2, 3, 8],
        ] {
            self.set(piece, 102 << 4, x, y, z);
        }
        self.torch(piece, Face::South, 2, 4, 7);
        self.torch(piece, Face::East, 1, 4, 6);
        self.torch(piece, Face::West, 3, 4, 6);
        self.torch(piece, Face::North, 2, 4, 5);
        let ladder = Self::faced(65, Face::West);
        for y in 1..=9 {
            self.set(piece, ladder, 3, y, 3);
        }
        self.set(piece, 0, 2, 1, 0);
        self.set(piece, 0, 2, 2, 0);
        self.door(piece, 2, 1, 0);
        self.step(piece, sn, 2);
        self.foundations(piece, 5, 9, 12, c);
    }

    fn hall(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 7, false) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let n = self.stair(piece, 53, Face::North);
        let s = self.stair(piece, 53, Face::South);
        let w = self.stair(piece, 53, Face::West);
        let pl = self.style(piece, 5 << 4);
        let log = self.style(piece, 17 << 4);
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 1, 1, 1, 7, 4, 4, 0);
        self.fill(piece, 2, 1, 6, 8, 4, 10, 0);
        self.fill(piece, 2, 0, 6, 8, 0, 10, 3 << 4);
        self.set(piece, c, 6, 0, 6);
        self.fill(piece, 2, 1, 6, 2, 1, 10, f);
        self.fill(piece, 8, 1, 6, 8, 1, 10, f);
        self.fill(piece, 3, 1, 10, 7, 1, 10, f);
        self.fill(piece, 1, 0, 1, 7, 0, 4, pl);
        self.fill(piece, 0, 0, 0, 0, 3, 5, c);
        self.fill(piece, 8, 0, 0, 8, 3, 5, c);
        self.fill(piece, 1, 0, 0, 7, 1, 0, c);
        self.fill(piece, 1, 0, 5, 7, 1, 5, c);
        self.fill(piece, 1, 2, 0, 7, 3, 0, pl);
        self.fill(piece, 1, 2, 5, 7, 3, 5, pl);
        self.fill(piece, 0, 4, 1, 8, 4, 1, pl);
        self.fill(piece, 0, 4, 4, 8, 4, 4, pl);
        self.fill(piece, 0, 5, 2, 8, 5, 3, pl);
        for [x, y, z] in [[0, 4, 2], [0, 4, 3], [8, 4, 2], [8, 4, 3]] {
            self.set(piece, pl, x, y, z);
        }
        for i in -1..=2 {
            for x in 0..=8 {
                self.set(piece, n, x, 4 + i, i);
                self.set(piece, s, x, 4 + i, 5 - i);
            }
        }
        for [x, y, z] in [[0, 2, 1], [0, 2, 4], [8, 2, 1], [8, 2, 4]] {
            self.set(piece, log, x, y, z);
        }
        for [x, y, z] in [
            [0, 2, 2],
            [0, 2, 3],
            [8, 2, 2],
            [8, 2, 3],
            [2, 2, 5],
            [3, 2, 5],
            [5, 2, 0],
            [6, 2, 5],
        ] {
            self.set(piece, 102 << 4, x, y, z);
        }
        self.set(piece, f, 2, 1, 3);
        self.set(piece, 72 << 4, 2, 2, 3);
        self.set(piece, pl, 1, 1, 4);
        self.set(piece, n, 2, 1, 4);
        self.set(piece, w, 1, 1, 3);
        self.fill(piece, 5, 0, 1, 7, 0, 3, 43 << 4);
        self.set(piece, 43 << 4, 6, 1, 1);
        self.set(piece, 43 << 4, 6, 1, 2);
        self.set(piece, 0, 2, 1, 0);
        self.set(piece, 0, 2, 2, 0);
        self.torch(piece, Face::North, 2, 3, 1);
        self.door(piece, 2, 1, 0);
        self.step(piece, n, 2);
        self.set(piece, 0, 6, 1, 5);
        self.set(piece, 0, 6, 2, 5);
        self.torch(piece, Face::South, 6, 3, 4);
        self.door(piece, 6, 1, 5);
        self.foundations(piece, 9, 5, 7, c);
    }

    fn house1(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 9, false) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let n = self.stair(piece, 53, Face::North);
        let s = self.stair(piece, 53, Face::South);
        let e = self.stair(piece, 53, Face::East);
        let pl = self.style(piece, 5 << 4);
        let step = self.stair(piece, 67, Face::North);
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 1, 1, 1, 7, 5, 4, 0);
        self.fill(piece, 0, 0, 0, 8, 0, 5, c);
        self.fill(piece, 0, 5, 0, 8, 5, 5, c);
        self.fill(piece, 0, 6, 1, 8, 6, 4, c);
        self.fill(piece, 0, 7, 2, 8, 7, 3, c);
        for i in -1..=2 {
            for x in 0..=8 {
                self.set(piece, n, x, 6 + i, i);
                self.set(piece, s, x, 6 + i, 5 - i);
            }
        }
        self.fill(piece, 0, 1, 0, 0, 1, 5, c);
        self.fill(piece, 1, 1, 5, 8, 1, 5, c);
        self.fill(piece, 8, 1, 0, 8, 1, 4, c);
        self.fill(piece, 2, 1, 0, 7, 1, 0, c);
        self.fill(piece, 0, 2, 0, 0, 4, 0, c);
        self.fill(piece, 0, 2, 5, 0, 4, 5, c);
        self.fill(piece, 8, 2, 5, 8, 4, 5, c);
        self.fill(piece, 8, 2, 0, 8, 4, 0, c);
        self.fill(piece, 0, 2, 1, 0, 4, 4, pl);
        self.fill(piece, 1, 2, 5, 7, 4, 5, pl);
        self.fill(piece, 8, 2, 1, 8, 4, 4, pl);
        self.fill(piece, 1, 2, 0, 7, 4, 0, pl);
        for [x, y, z] in [
            [4, 2, 0],
            [5, 2, 0],
            [6, 2, 0],
            [4, 3, 0],
            [5, 3, 0],
            [6, 3, 0],
            [0, 2, 2],
            [0, 2, 3],
            [0, 3, 2],
            [0, 3, 3],
            [8, 2, 2],
            [8, 2, 3],
            [8, 3, 2],
            [8, 3, 3],
            [2, 2, 5],
            [3, 2, 5],
            [5, 2, 5],
            [6, 2, 5],
        ] {
            self.set(piece, 102 << 4, x, y, z);
        }
        self.fill(piece, 1, 4, 1, 7, 4, 1, pl);
        self.fill(piece, 1, 4, 4, 7, 4, 4, pl);
        self.fill(piece, 1, 3, 4, 7, 3, 4, 47 << 4);
        self.set(piece, pl, 7, 1, 4);
        self.set(piece, e, 7, 1, 3);
        for x in [6, 5, 4, 3] {
            self.set(piece, n, x, 1, 4);
        }
        self.set(piece, f, 6, 1, 3);
        self.set(piece, 72 << 4, 6, 2, 3);
        self.set(piece, f, 4, 1, 3);
        self.set(piece, 72 << 4, 4, 2, 3);
        self.set(piece, 58 << 4, 7, 1, 1);
        self.set(piece, 0, 1, 1, 0);
        self.set(piece, 0, 1, 2, 0);
        self.door(piece, 1, 1, 0);
        self.step(piece, step, 1);
        self.foundations(piece, 9, 6, 9, c);
    }

    fn house2(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 6, false) {
            return;
        }
        let c = 4 << 4;
        let n = self.stair(piece, 53, Face::North);
        let w = self.stair(piece, 53, Face::West);
        let pl = self.style(piece, 5 << 4);
        let step = self.stair(piece, 67, Face::North);
        let log = self.style(piece, 17 << 4);
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 0, 1, 0, 9, 4, 6, 0);
        self.fill(piece, 0, 0, 0, 9, 0, 6, c);
        self.fill(piece, 0, 4, 0, 9, 4, 6, c);
        self.fill(piece, 0, 5, 0, 9, 5, 6, 44 << 4);
        self.fill(piece, 1, 5, 1, 8, 5, 5, 0);
        self.fill(piece, 1, 1, 0, 2, 3, 0, pl);
        self.fill(piece, 0, 1, 0, 0, 4, 0, log);
        self.fill(piece, 3, 1, 0, 3, 4, 0, log);
        self.fill(piece, 0, 1, 6, 0, 4, 6, log);
        self.set(piece, pl, 3, 3, 1);
        self.fill(piece, 3, 1, 2, 3, 3, 2, pl);
        self.fill(piece, 4, 1, 3, 5, 3, 3, pl);
        self.fill(piece, 0, 1, 1, 0, 3, 5, pl);
        self.fill(piece, 1, 1, 6, 5, 3, 6, pl);
        self.fill(piece, 5, 1, 0, 5, 3, 0, f);
        self.fill(piece, 9, 1, 0, 9, 3, 0, f);
        self.fill(piece, 6, 1, 4, 9, 4, 6, c);
        self.set(piece, 10 << 4, 7, 1, 5);
        self.set(piece, 10 << 4, 8, 1, 5);
        self.set(piece, 101 << 4, 9, 2, 5);
        self.set(piece, 101 << 4, 9, 2, 4);
        self.fill(piece, 7, 2, 4, 8, 2, 5, 0);
        self.set(piece, c, 6, 1, 3);
        let furnace = Self::faced(61, Face::North);
        self.set(piece, furnace, 6, 2, 3);
        self.set(piece, furnace, 6, 3, 3);
        self.set(piece, 43 << 4, 8, 1, 1);
        for [x, y, z] in [[0, 2, 2], [0, 2, 4], [2, 2, 6], [4, 2, 6]] {
            self.set(piece, 102 << 4, x, y, z);
        }
        self.set(piece, f, 2, 1, 4);
        self.set(piece, 72 << 4, 2, 2, 4);
        self.set(piece, pl, 1, 1, 5);
        self.set(piece, n, 2, 1, 5);
        self.set(piece, w, 1, 1, 4);
        if !piece.chest {
            let q = self.pos(piece, 5, 1, 5);
            if self.inside(q) {
                piece.chest = true;
                let state = self.chest_state(piece, q);
                let _loot_seed = self.random.next_long();
                self.set_world(q, state);
            }
        }
        for x in 6..=8 {
            self.step(piece, step, x);
        }
        self.foundations(piece, 10, 7, 6, c);
    }

    fn house3(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 7, false) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let n = self.stair(piece, 53, Face::North);
        let s = self.stair(piece, 53, Face::South);
        let e = self.stair(piece, 53, Face::East);
        let w = self.stair(piece, 53, Face::West);
        let pl = self.style(piece, 5 << 4);
        let log = self.style(piece, 17 << 4);
        self.fill(piece, 1, 1, 1, 7, 4, 4, 0);
        self.fill(piece, 2, 1, 6, 8, 4, 10, 0);
        self.fill(piece, 2, 0, 5, 8, 0, 10, pl);
        self.fill(piece, 1, 0, 1, 7, 0, 4, pl);
        self.fill(piece, 0, 0, 0, 0, 3, 5, c);
        self.fill(piece, 8, 0, 0, 8, 3, 10, c);
        self.fill(piece, 1, 0, 0, 7, 2, 0, c);
        self.fill(piece, 1, 0, 5, 2, 1, 5, c);
        self.fill(piece, 2, 0, 6, 2, 3, 10, c);
        self.fill(piece, 3, 0, 10, 7, 3, 10, c);
        self.fill(piece, 1, 2, 0, 7, 3, 0, pl);
        self.fill(piece, 1, 2, 5, 2, 3, 5, pl);
        self.fill(piece, 0, 4, 1, 8, 4, 1, pl);
        self.fill(piece, 0, 4, 4, 3, 4, 4, pl);
        self.fill(piece, 0, 5, 2, 8, 5, 3, pl);
        for [x, y, z] in [[0, 4, 2], [0, 4, 3], [8, 4, 2], [8, 4, 3], [8, 4, 4]] {
            self.set(piece, pl, x, y, z);
        }
        for i in -1..=2 {
            for x in 0..=8 {
                self.set(piece, n, x, 4 + i, i);
                if (i > -1 || x <= 1) && (i > 0 || x <= 3) && (i > 1 || x <= 4 || x >= 6) {
                    self.set(piece, s, x, 4 + i, 5 - i);
                }
            }
        }
        self.fill(piece, 3, 4, 5, 3, 4, 10, pl);
        self.fill(piece, 7, 4, 2, 7, 4, 10, pl);
        self.fill(piece, 4, 5, 4, 4, 5, 10, pl);
        self.fill(piece, 6, 5, 4, 6, 5, 10, pl);
        self.fill(piece, 5, 6, 3, 5, 6, 10, pl);
        for x in (1..=4).rev() {
            self.set(piece, pl, x, 2 + x, 7 - x);
            for z in 8 - x..=10 {
                self.set(piece, e, x, 2 + x, z);
            }
        }
        self.set(piece, pl, 6, 6, 3);
        self.set(piece, pl, 7, 5, 4);
        self.set(piece, w, 6, 6, 4);
        for x in 6..=8 {
            for z in 5..=10 {
                self.set(piece, w, x, 12 - x, z);
            }
        }
        for [x, y, z] in [
            [0, 2, 1],
            [0, 2, 4],
            [4, 2, 0],
            [6, 2, 0],
            [8, 2, 1],
            [8, 2, 4],
            [8, 2, 6],
            [8, 2, 9],
            [2, 2, 6],
            [2, 2, 9],
            [4, 4, 10],
            [6, 4, 10],
        ] {
            self.set(piece, log, x, y, z);
        }
        for [x, y, z] in [
            [0, 2, 2],
            [0, 2, 3],
            [5, 2, 0],
            [8, 2, 2],
            [8, 2, 3],
            [8, 2, 7],
            [8, 2, 8],
            [2, 2, 7],
            [2, 2, 8],
            [5, 4, 10],
        ] {
            self.set(piece, 102 << 4, x, y, z);
        }
        self.set(piece, pl, 8, 2, 5);
        self.set(piece, pl, 5, 5, 10);
        self.set(piece, 0, 2, 1, 0);
        self.set(piece, 0, 2, 2, 0);
        self.torch(piece, Face::North, 2, 3, 1);
        self.door(piece, 2, 1, 0);
        self.fill(piece, 1, 0, -1, 3, 2, -1, 0);
        self.step(piece, n, 2);
        for z in 0..5 {
            for x in 0..9 {
                self.clear_up(piece, x, 7, z);
                self.down(piece, c, x, -1, z);
            }
        }
        for z in 5..11 {
            for x in 2..9 {
                self.clear_up(piece, x, 7, z);
                self.down(piece, c, x, -1, z);
            }
        }
    }

    fn house4(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 6, false) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let pl = self.style(piece, 5 << 4);
        let step = self.stair(piece, 67, Face::North);
        let log = self.style(piece, 17 << 4);
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 0, 0, 0, 4, 0, 4, c);
        self.fill(piece, 0, 4, 0, 4, 4, 4, log);
        self.fill(piece, 1, 4, 1, 3, 4, 3, pl);
        for [x, y, z] in [
            [0, 1, 0],
            [0, 2, 0],
            [0, 3, 0],
            [4, 1, 0],
            [4, 2, 0],
            [4, 3, 0],
            [0, 1, 4],
            [0, 2, 4],
            [0, 3, 4],
            [4, 1, 4],
            [4, 2, 4],
            [4, 3, 4],
        ] {
            self.set(piece, c, x, y, z);
        }
        self.fill(piece, 0, 1, 1, 0, 3, 3, pl);
        self.fill(piece, 4, 1, 1, 4, 3, 3, pl);
        self.fill(piece, 1, 1, 4, 3, 3, 4, pl);
        for [x, y, z] in [[0, 2, 2], [2, 2, 4], [4, 2, 2]] {
            self.set(piece, 102 << 4, x, y, z);
        }
        for [x, y, z] in [
            [1, 1, 0],
            [1, 2, 0],
            [1, 3, 0],
            [2, 3, 0],
            [3, 3, 0],
            [3, 2, 0],
            [3, 1, 0],
        ] {
            self.set(piece, pl, x, y, z);
        }
        self.step(piece, step, 2);
        self.fill(piece, 1, 1, 1, 3, 3, 3, 0);
        if (piece.variant & 1) != 0 {
            for x in 0..=4 {
                self.set(piece, f, x, 5, 0);
                self.set(piece, f, x, 5, 4);
            }
            for z in 1..=3 {
                self.set(piece, f, 4, 5, z);
                self.set(piece, f, 0, 5, z);
            }
            let ladder = Self::faced(65, Face::South);
            for y in 1..=4 {
                self.set(piece, ladder, 3, y, 3);
            }
        }
        self.torch(piece, Face::North, 2, 3, 1);
        self.foundations(piece, 5, 5, 6, c);
    }

    fn wood_hut(&mut self, piece: &mut VillagePiece) {
        if !self.prepare(piece, 6, false) {
            return;
        }
        let c = self.style(piece, 4 << 4);
        let pl = self.style(piece, 5 << 4);
        let step = self.stair(piece, 67, Face::North);
        let log = self.style(piece, 17 << 4);
        let f = self.style(piece, 85 << 4);
        self.fill(piece, 1, 1, 1, 3, 5, 4, 0);
        self.fill(piece, 0, 0, 0, 3, 0, 4, c);
        self.fill(piece, 1, 0, 1, 2, 0, 3, 3 << 4);
        if (piece.variant & 1) != 0 {
            self.fill(piece, 1, 4, 1, 2, 4, 3, log);
        } else {
            self.fill(piece, 1, 5, 1, 2, 5, 3, log);
        }
        for [x, y, z] in [
            [1, 4, 0],
            [2, 4, 0],
            [1, 4, 4],
            [2, 4, 4],
            [0, 4, 1],
            [0, 4, 2],
            [0, 4, 3],
            [3, 4, 1],
            [3, 4, 2],
            [3, 4, 3],
        ] {
            self.set(piece, log, x, y, z);
        }
        self.fill(piece, 0, 1, 0, 0, 3, 0, log);
        self.fill(piece, 3, 1, 0, 3, 3, 0, log);
        self.fill(piece, 0, 1, 4, 0, 3, 4, log);
        self.fill(piece, 3, 1, 4, 3, 3, 4, log);
        self.fill(piece, 0, 1, 1, 0, 3, 3, pl);
        self.fill(piece, 3, 1, 1, 3, 3, 3, pl);
        self.fill(piece, 1, 1, 0, 2, 3, 0, pl);
        self.fill(piece, 1, 1, 4, 2, 3, 4, pl);
        self.set(piece, 102 << 4, 0, 2, 2);
        self.set(piece, 102 << 4, 3, 2, 2);
        let table = piece.variant >> 1;
        if table > 0 {
            self.set(piece, f, table, 1, 3);
            self.set(piece, 72 << 4, table, 2, 3);
        }
        self.set(piece, 0, 1, 1, 0);
        self.set(piece, 0, 1, 2, 0);
        self.door(piece, 1, 1, 0);
        self.step(piece, step, 1);
        self.foundations(piece, 4, 5, 6, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;
    use crate::level::generator::realistic_terrain::structure::topology::component_box;

    struct FlatHeight(i32);

    impl TerrainHeightProvider for FlatHeight {
        fn terrain_height(&mut self, _x: i32, _z: i32) -> i32 {
            self.0
        }
    }

    #[derive(Default)]
    struct RecordingHeight {
        calls: Vec<(i32, i32)>,
    }

    impl TerrainHeightProvider for RecordingHeight {
        fn terrain_height(&mut self, x: i32, z: i32) -> i32 {
            self.calls.push((x, z));
            match (x, z) {
                (8, 8) => 70,
                (9, 9) => 80,
                _ => 40,
            }
        }
    }

    fn test_chunks() -> HashMap<(i32, i32), Chunk> {
        let mut chunks = HashMap::new();
        for x in 0..=1 {
            for z in 0..=1 {
                chunks.insert((x, z), Chunk::empty(x, z));
            }
        }
        chunks
    }

    fn fill_ground(world: &mut GenWorld) {
        for x in 0..32 {
            for z in 0..32 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 2 << 4 } else { 3 << 4 });
                }
            }
        }
    }

    fn dimensions(kind: PieceKind) -> (i32, i32, i32) {
        match kind {
            PieceKind::Start => (6, 15, 6),
            PieceKind::Path => (3, 3, 7),
            PieceKind::Torch => (3, 4, 2),
            PieceKind::Field1 => (13, 4, 9),
            PieceKind::Field2 => (7, 4, 9),
            PieceKind::Church => (5, 12, 9),
            PieceKind::Hall => (9, 7, 11),
            PieceKind::House1 => (9, 9, 6),
            PieceKind::House2 => (10, 6, 7),
            PieceKind::House3 => (9, 7, 12),
            PieceKind::House4Garden => (5, 6, 5),
            PieceKind::WoodHut => (4, 6, 5),
        }
    }

    fn manual_start(kind: PieceKind, direction: Direction, style: i32) -> Arc<VillageStructure> {
        let (width, height, depth) = dimensions(kind);
        let bounding_box = component_box(8, 64, 8, width, height, depth, direction);
        let mut piece = VillagePiece::new(kind, 0, bounding_box, direction, style, false);
        piece.crops = match kind {
            PieceKind::Field1 => vec![0, 2, 4, 9],
            PieceKind::Field2 => vec![4, 2],
            _ => Vec::new(),
        };
        piece.variant = 1 | (2 << 1);
        Arc::new(VillageStructure {
            pieces: vec![piece],
            bounding_box,
            valid: true,
            structure_type: style,
            zombie: false,
            non_road_count: 1,
            chunk_x: 0,
            chunk_z: 0,
        })
    }

    #[test]
    fn prepare_reads_the_whole_unclipped_footprint() {
        let mut chunks = test_chunks();
        let mut world = GenWorld::new(&mut chunks);
        let mut terrain = RecordingHeight::default();
        let mut random = JavaRandom::new(1);
        let mut placer = Placer {
            world: &mut world,
            terrain: &mut terrain,
            random: &mut random,
            clip: population_clip(0, 0),
        };
        let bounding_box = BoundingBox::new(6, 64, 8, 9, 69, 9);
        let mut piece = VillagePiece::new(
            PieceKind::House2,
            0,
            bounding_box,
            Direction::South,
            0,
            false,
        );
        assert!(placer.prepare(&mut piece, 6, false));
        assert_eq!(terrain.calls.len(), 8);
        assert!(
            terrain.calls.contains(&(6, 8)),
            "裁剪区外的 footprint 也必须取样"
        );
        assert_eq!(piece.average_ground, (62 * 6 + 70 + 80) / 8);
        assert_eq!(piece.bounding_box.min_y, piece.average_ground);
    }

    #[test]
    fn rotation_and_all_four_styles_match_reference_rules() {
        let mut chunks = test_chunks();
        let mut world = GenWorld::new(&mut chunks);
        let mut terrain = FlatHeight(63);
        let mut random = JavaRandom::new(1);
        let placer = Placer {
            world: &mut world,
            terrain: &mut terrain,
            random: &mut random,
            clip: population_clip(0, 0),
        };
        let bounding_box = BoundingBox::new(8, 63, 8, 12, 68, 12);
        for (direction, stair_meta, chest_meta) in [
            (Direction::North, 3, 2),
            (Direction::East, 0, 5),
            (Direction::South, 2, 3),
            (Direction::West, 1, 4),
        ] {
            let piece = VillagePiece::new(PieceKind::House1, 0, bounding_box, direction, 0, false);
            assert_eq!(
                placer.rotate(&piece, Placer::<FlatHeight>::faced(67, Face::North)) & 3,
                stair_meta
            );
            assert_eq!(
                placer.rotate(&piece, Placer::<FlatHeight>::faced(54, Face::North)) & 15,
                chest_meta
            );
        }
        let mut piece = VillagePiece::new(
            PieceKind::House1,
            0,
            bounding_box,
            Direction::North,
            0,
            false,
        );
        assert_eq!(placer.style(&piece, 4 << 4), 4 << 4);
        piece.structure_type = 1;
        assert_eq!(placer.style(&piece, 4 << 4), 24 << 4);
        assert_eq!(placer.style(&piece, 5 << 4), (24 << 4) | 2);
        assert_eq!(placer.style(&piece, 67 << 4), 128 << 4);
        piece.structure_type = 2;
        assert_eq!(placer.style(&piece, 4 << 4), 162 << 4);
        assert_eq!(placer.style(&piece, 85 << 4), 192 << 4);
        piece.structure_type = 3;
        assert_eq!(placer.style(&piece, 17 << 4), (17 << 4) | 1);
        assert_eq!(placer.style(&piece, 85 << 4), 188 << 4);
    }

    #[test]
    fn every_component_places_without_mutating_cached_topology() {
        let kinds = [
            PieceKind::Start,
            PieceKind::Path,
            PieceKind::Torch,
            PieceKind::Field1,
            PieceKind::Field2,
            PieceKind::Church,
            PieceKind::Hall,
            PieceKind::House1,
            PieceKind::House2,
            PieceKind::House3,
            PieceKind::House4Garden,
            PieceKind::WoodHut,
        ];
        for (index, kind) in kinds.into_iter().enumerate() {
            let start = manual_start(kind, Direction::South, index as i32 % 4);
            let original = start.as_ref().clone();
            let mut chunks = test_chunks();
            let mut world = GenWorld::new(&mut chunks);
            fill_ground(&mut world);
            let mut terrain = FlatHeight(63);
            let mut random = JavaRandom::new(99);
            assert!(generate(
                &mut world,
                &mut terrain,
                &mut random,
                std::slice::from_ref(&start),
                0,
                0
            ));
            let changed = (64..90)
                .any(|y| (8..24).any(|x| (7..24).any(|z| world.block(x, y, z) != 0)))
                || (8..24).any(|x| (8..24).any(|z| world.block(x, 63, z) == 198));
            assert!(changed, "组件 {kind:?} 没有放置方块");
            assert_eq!(start.as_ref(), &original, "缓存拓扑被 {kind:?} 放置修改");
            if kind == PieceKind::House2 {
                assert_eq!(world.block(13, 64, 13), 54);
                assert!((2..=5).contains(&world.meta(13, 64, 13)));
            }
        }
    }

    #[test]
    fn path_and_step_translate_native_states_in_both_directions() {
        let start = manual_start(PieceKind::Path, Direction::South, 0);
        let mut chunks = test_chunks();
        let mut world = GenWorld::new(&mut chunks);
        fill_ground(&mut world);
        let mut terrain = FlatHeight(63);
        let mut random = JavaRandom::new(1);
        generate(&mut world, &mut terrain, &mut random, &[start], 0, 0);
        assert_eq!(
            world.block(8, 63, 8),
            198,
            "Java grass_path 208 应写成 native 198"
        );
        assert_eq!(to_java(world.block_state(8, 63, 8)), 208 << 4);

        let mut placer = Placer {
            world: &mut world,
            terrain: &mut terrain,
            random: &mut random,
            clip: population_clip(0, 0),
        };
        let bounding_box = component_box(8, 63, 10, 5, 6, 5, Direction::South);
        let piece = VillagePiece::new(
            PieceKind::House4Garden,
            0,
            bounding_box,
            Direction::South,
            0,
            false,
        );
        let q = placer.pos(&piece, 2, -1, -1);
        placer.set_world(q, 208 << 4);
        placer.set_world(q.up(), 0);
        placer.step(&piece, Placer::<FlatHeight>::faced(67, Face::North), 2);
        assert_eq!(
            world.block(q.x, q.y, q.z),
            2,
            "读取 native path 后应还原成 Java 208"
        );
        assert_eq!(world.block(q.x, q.y + 1, q.z), 67);
    }

    #[test]
    fn crop_rng_order_and_chest_long_are_preserved() {
        let field = manual_start(PieceKind::Field1, Direction::South, 0);
        let mut chunks = test_chunks();
        let mut world = GenWorld::new(&mut chunks);
        fill_ground(&mut world);
        let mut terrain = FlatHeight(63);
        let mut random = JavaRandom::new(0x1234);
        generate(&mut world, &mut terrain, &mut random, &[field], 0, 0);
        let mut expected = JavaRandom::new(0x1234);
        for _z in 1..=7 {
            for bound in [8, 8, 8, 8, 3, 3, 8, 8] {
                expected.next_int_bound(bound);
            }
        }
        assert_eq!(random.state(), expected.state());
        assert!((8..24).any(|x| (8..24).any(|z| (64..70).any(|y| world.block(x, y, z) == 244))));

        let house = manual_start(PieceKind::House2, Direction::South, 0);
        let mut chunks = test_chunks();
        let mut world = GenWorld::new(&mut chunks);
        fill_ground(&mut world);
        let mut random = JavaRandom::new(77);
        generate(
            &mut world,
            &mut terrain,
            &mut random,
            std::slice::from_ref(&house),
            0,
            0,
        );
        let mut expected = JavaRandom::new(77);
        expected.next_long();
        assert_eq!(random.state(), expected.state());
        assert_eq!(world.block(13, 64, 13), 54);
        assert!(!house.pieces[0].chest, "箱子状态不能写回缓存");
    }

    #[test]
    fn replaying_one_cached_source_is_deterministic() {
        let start = manual_start(PieceKind::House2, Direction::South, 0);
        let mut snapshots = Vec::new();
        let mut states = Vec::new();
        for _ in 0..2 {
            let mut chunks = test_chunks();
            let mut world = GenWorld::new(&mut chunks);
            fill_ground(&mut world);
            let mut terrain = FlatHeight(63);
            let mut random = JavaRandom::new(9182);
            generate(
                &mut world,
                &mut terrain,
                &mut random,
                std::slice::from_ref(&start),
                0,
                0,
            );
            let mut snapshot = Vec::new();
            for y in 55..80 {
                for x in 0..32 {
                    for z in 0..32 {
                        snapshot.push(world.block_state(x, y, z));
                    }
                }
            }
            snapshots.push(snapshot);
            states.push(random.state());
        }
        assert_eq!(snapshots[0], snapshots[1]);
        assert_eq!(states[0], states[1]);
    }
}
