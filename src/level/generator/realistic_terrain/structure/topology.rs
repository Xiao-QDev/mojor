//! Minecraft 1.12 `StructureVillagePieces` 的不可变拓扑构建。

use crate::level::generator::realistic_terrain::biome::BiomeProvider;
use crate::level::generator::realistic_terrain::math::JavaRandom;

use super::placement::BoundingBox;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    const HORIZONTAL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    fn random(random: &mut JavaRandom) -> Self {
        Self::HORIZONTAL[random.next_int_bound(4) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PieceKind {
    Start,
    Path,
    Torch,
    Field1,
    Field2,
    Church,
    Hall,
    House1,
    House2,
    House3,
    House4Garden,
    WoodHut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VillagePiece {
    pub kind: PieceKind,
    pub component_type: i32,
    pub bounding_box: BoundingBox,
    pub direction: Direction,
    pub structure_type: i32,
    pub zombie: bool,
    pub variant: i32,
    pub crops: Vec<i32>,
    /// 放置期 HPos；拓扑缓存始终保持 -1。
    pub average_ground: i32,
    /// House2 的箱子每次放置只生成一次。
    pub chest: bool,
}

impl VillagePiece {
    pub fn new(
        kind: PieceKind,
        component_type: i32,
        bounding_box: BoundingBox,
        direction: Direction,
        structure_type: i32,
        zombie: bool,
    ) -> Self {
        Self {
            kind,
            component_type,
            bounding_box,
            direction,
            structure_type,
            zombie,
            variant: 0,
            crops: Vec::new(),
            average_ground: -1,
            chest: false,
        }
    }

    pub fn path_length(&self) -> i32 {
        self.bounding_box.x_size().max(self.bounding_box.z_size())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VillageStructure {
    pub pieces: Vec<VillagePiece>,
    pub bounding_box: BoundingBox,
    pub valid: bool,
    pub structure_type: i32,
    pub zombie: bool,
    pub non_road_count: i32,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

#[derive(Clone, Debug)]
struct PieceWeight {
    kind: PieceKind,
    weight: i32,
    limit: i32,
    spawned: i32,
}

struct Construction {
    pieces: Vec<VillagePiece>,
    weights: Vec<PieceWeight>,
    last_weight: Option<PieceKind>,
    pending_houses: Vec<usize>,
    pending_roads: Vec<usize>,
    start_box: BoundingBox,
    terrain_type: i32,
    structure_type: i32,
    zombie: bool,
}

impl VillageStructure {
    pub fn create(
        random: &mut JavaRandom,
        provider: &mut BiomeProvider,
        chunk_x: i32,
        chunk_z: i32,
        size: i32,
    ) -> Self {
        let x = chunk_x.wrapping_shl(4).wrapping_add(2);
        let z = chunk_z.wrapping_shl(4).wrapping_add(2);
        let structure_type = structure_type(provider.get_biome(x, z));
        let mut result = Self::create_at(random, x, z, size, structure_type);
        result.chunk_x = chunk_x;
        result.chunk_z = chunk_z;
        result
    }

    pub fn create_at(
        random: &mut JavaRandom,
        x: i32,
        z: i32,
        size: i32,
        structure_type: i32,
    ) -> Self {
        let size = size.max(0);
        let weights = Self::weights(random, size);
        let direction = Direction::random(random);
        let zombie = random.next_int_bound(50) == 0;
        let start_box = BoundingBox::new(x, 64, z, x + 5, 78, z + 5);
        let start = VillagePiece::new(
            PieceKind::Start,
            0,
            start_box,
            direction,
            structure_type,
            zombie,
        );
        let mut construction = Construction {
            pieces: vec![start],
            weights,
            last_weight: None,
            pending_houses: Vec::new(),
            pending_roads: Vec::new(),
            start_box,
            terrain_type: size,
            structure_type,
            zombie,
        };
        construction.build_well(random);
        while !construction.pending_roads.is_empty() || !construction.pending_houses.is_empty() {
            let index = if construction.pending_roads.is_empty() {
                let selected =
                    random.next_int_bound(construction.pending_houses.len() as i32) as usize;
                construction.pending_houses.remove(selected)
            } else {
                let selected =
                    random.next_int_bound(construction.pending_roads.len() as i32) as usize;
                construction.pending_roads.remove(selected)
            };
            construction.build_piece(index, random);
        }

        let mut bounding_box = construction.pieces[0].bounding_box;
        for piece in &construction.pieces[1..] {
            bounding_box.expand_to(&piece.bounding_box);
        }
        let non_road_count = construction
            .pieces
            .iter()
            .filter(|piece| piece.kind != PieceKind::Path)
            .count() as i32;
        Self {
            pieces: construction.pieces,
            bounding_box,
            valid: non_road_count > 2,
            structure_type,
            zombie,
            non_road_count,
            chunk_x: 0,
            chunk_z: 0,
        }
    }

    fn weights(random: &mut JavaRandom, size: i32) -> Vec<PieceWeight> {
        use PieceKind::*;
        let definitions = [
            (House4Garden, 4, 2 + size, 4 + size * 2),
            (Church, 20, size, 1 + size),
            (House1, 20, size, 2 + size),
            (WoodHut, 3, 2 + size, 5 + size * 3),
            (Hall, 15, size, 2 + size),
            (Field1, 3, 1 + size, 4 + size),
            (Field2, 3, 2 + size, 4 + size * 2),
            (House2, 15, 0, 1 + size),
            (House3, 8, size, 3 + size * 2),
        ];
        let mut result = Vec::new();
        for (kind, weight, min, max) in definitions {
            let limit = min + random.next_int_bound(max - min + 1);
            if limit != 0 {
                result.push(PieceWeight {
                    kind,
                    weight,
                    limit,
                    spawned: 0,
                });
            }
        }
        result
    }
}

pub fn structure_type(biome: i32) -> i32 {
    if [2, 17, 130].contains(&biome) {
        1
    } else if [35, 36, 163, 164].contains(&biome) {
        2
    } else if [5, 19, 30, 31, 32, 33, 158, 160, 161].contains(&biome) {
        3
    } else {
        0
    }
}

pub fn component_box(
    x: i32,
    y: i32,
    z: i32,
    width: i32,
    height: i32,
    depth: i32,
    direction: Direction,
) -> BoundingBox {
    match direction {
        Direction::North => BoundingBox::new(x, y, z - depth + 1, x + width - 1, y + height - 1, z),
        Direction::South => BoundingBox::new(x, y, z, x + width - 1, y + height - 1, z + depth - 1),
        Direction::West => BoundingBox::new(x - depth + 1, y, z, x, y + height - 1, z + width - 1),
        Direction::East => BoundingBox::new(x, y, z, x + depth - 1, y + height - 1, z + width - 1),
    }
}

fn intersects(pieces: &[VillagePiece], bounding_box: BoundingBox) -> bool {
    pieces
        .iter()
        .any(|piece| piece.bounding_box.intersects(&bounding_box))
}

impl Construction {
    fn build_piece(&mut self, index: usize, random: &mut JavaRandom) {
        if self.pieces[index].kind == PieceKind::Path {
            self.build_path(index, random);
        }
    }

    fn build_well(&mut self, random: &mut JavaRandom) {
        let b = self.start_box;
        self.add_road(
            random,
            b.min_x - 1,
            b.max_y - 4,
            b.min_z + 1,
            Direction::West,
            0,
        );
        self.add_road(
            random,
            b.max_x + 1,
            b.max_y - 4,
            b.min_z + 1,
            Direction::East,
            0,
        );
        self.add_road(
            random,
            b.min_x + 1,
            b.max_y - 4,
            b.min_z - 1,
            Direction::North,
            0,
        );
        self.add_road(
            random,
            b.min_x + 1,
            b.max_y - 4,
            b.max_z + 1,
            Direction::South,
            0,
        );
    }

    fn build_path(&mut self, index: usize, random: &mut JavaRandom) {
        let piece = self.pieces[index].clone();
        let mut flag = false;
        let mut i = random.next_int_bound(5);
        while i < piece.path_length() - 8 {
            let next = self.next_nn(&piece, random, 0, i);
            if let Some(next) = next {
                i += next.bounding_box.x_size().max(next.bounding_box.z_size());
                flag = true;
            }
            i += 2 + random.next_int_bound(5);
        }
        let mut j = random.next_int_bound(5);
        while j < piece.path_length() - 8 {
            let next = self.next_pp(&piece, random, 0, j);
            if let Some(next) = next {
                j += next.bounding_box.x_size().max(next.bounding_box.z_size());
                flag = true;
            }
            j += 2 + random.next_int_bound(5);
        }
        let b = piece.bounding_box;
        let d = piece.direction;
        let t = piece.component_type;
        if flag && random.next_int_bound(3) > 0 {
            match d {
                Direction::North => {
                    self.add_road(random, b.min_x - 1, b.min_y, b.min_z, Direction::West, t)
                }
                Direction::South => self.add_road(
                    random,
                    b.min_x - 1,
                    b.min_y,
                    b.max_z - 2,
                    Direction::West,
                    t,
                ),
                Direction::West => {
                    self.add_road(random, b.min_x, b.min_y, b.min_z - 1, Direction::North, t)
                }
                Direction::East => self.add_road(
                    random,
                    b.max_x - 2,
                    b.min_y,
                    b.min_z - 1,
                    Direction::North,
                    t,
                ),
            };
        }
        if flag && random.next_int_bound(3) > 0 {
            match d {
                Direction::North => {
                    self.add_road(random, b.max_x + 1, b.min_y, b.min_z, Direction::East, t)
                }
                Direction::South => self.add_road(
                    random,
                    b.max_x + 1,
                    b.min_y,
                    b.max_z - 2,
                    Direction::East,
                    t,
                ),
                Direction::West => {
                    self.add_road(random, b.min_x, b.min_y, b.max_z + 1, Direction::South, t)
                }
                Direction::East => self.add_road(
                    random,
                    b.max_x - 2,
                    b.min_y,
                    b.max_z + 1,
                    Direction::South,
                    t,
                ),
            };
        }
    }

    fn next_nn(
        &mut self,
        piece: &VillagePiece,
        random: &mut JavaRandom,
        dy: i32,
        offset: i32,
    ) -> Option<VillagePiece> {
        let b = piece.bounding_box;
        let t = piece.component_type;
        if matches!(piece.direction, Direction::North | Direction::South) {
            self.add_house(
                random,
                b.min_x - 1,
                b.min_y + dy,
                b.min_z + offset,
                Direction::West,
                t,
            )
        } else {
            self.add_house(
                random,
                b.min_x + offset,
                b.min_y + dy,
                b.min_z - 1,
                Direction::North,
                t,
            )
        }
    }

    fn next_pp(
        &mut self,
        piece: &VillagePiece,
        random: &mut JavaRandom,
        dy: i32,
        offset: i32,
    ) -> Option<VillagePiece> {
        let b = piece.bounding_box;
        let t = piece.component_type;
        if matches!(piece.direction, Direction::North | Direction::South) {
            self.add_house(
                random,
                b.max_x + 1,
                b.min_y + dy,
                b.min_z + offset,
                Direction::East,
                t,
            )
        } else {
            self.add_house(
                random,
                b.min_x + offset,
                b.min_y + dy,
                b.max_z + 1,
                Direction::South,
                t,
            )
        }
    }

    fn add_house(
        &mut self,
        random: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        direction: Direction,
        component_type: i32,
    ) -> Option<VillagePiece> {
        if component_type > 50
            || (x - self.start_box.min_x).abs() > 112
            || (z - self.start_box.min_z).abs() > 112
        {
            return None;
        }
        let piece = self.generate_house(random, x, y, z, direction, component_type + 1);
        if let Some(ref piece) = piece {
            self.pieces.push(piece.clone());
            self.pending_houses.push(self.pieces.len() - 1);
        }
        piece
    }

    fn generate_house(
        &mut self,
        random: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        direction: Direction,
        component_type: i32,
    ) -> Option<VillagePiece> {
        let mut total = 0;
        let mut available = false;
        for weight in &self.weights {
            if weight.limit > 0 && weight.spawned < weight.limit {
                available = true;
            }
            total += weight.weight;
        }
        if !available || total <= 0 {
            return None;
        }
        for _ in 0..5 {
            let mut choice = random.next_int_bound(total);
            let mut weight_index = 0;
            while weight_index < self.weights.len() {
                let weight = &self.weights[weight_index];
                choice -= weight.weight;
                if choice < 0 {
                    if weight.spawned >= weight.limit
                        || (self.last_weight == Some(weight.kind) && self.weights.len() > 1)
                    {
                        break;
                    }
                    let kind = weight.kind;
                    let piece =
                        self.create_house_piece(kind, random, x, y, z, direction, component_type);
                    if let Some(piece) = piece {
                        self.weights[weight_index].spawned += 1;
                        self.last_weight = Some(kind);
                        if self.weights[weight_index].spawned >= self.weights[weight_index].limit {
                            self.weights.remove(weight_index);
                        }
                        return Some(piece);
                    }
                }
                weight_index += 1;
            }
        }
        let bounding_box = component_box(x, y, z, 3, 4, 2, direction);
        if intersects(&self.pieces, bounding_box) {
            return None;
        }
        Some(VillagePiece::new(
            PieceKind::Torch,
            component_type,
            bounding_box,
            direction,
            self.structure_type,
            self.zombie,
        ))
    }

    fn create_house_piece(
        &self,
        kind: PieceKind,
        random: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        direction: Direction,
        component_type: i32,
    ) -> Option<VillagePiece> {
        let (width, height, depth, low_check) = match kind {
            PieceKind::House4Garden => (5, 6, 5, false),
            PieceKind::Church => (5, 12, 9, true),
            PieceKind::House1 => (9, 9, 6, true),
            PieceKind::WoodHut => (4, 6, 5, true),
            PieceKind::Hall => (9, 7, 11, true),
            PieceKind::Field1 => (13, 4, 9, true),
            PieceKind::Field2 => (7, 4, 9, true),
            PieceKind::House2 => (10, 6, 7, true),
            PieceKind::House3 => (9, 7, 12, true),
            _ => return None,
        };
        let bounding_box = component_box(x, y, z, width, height, depth, direction);
        if (low_check && bounding_box.min_y <= 10) || intersects(&self.pieces, bounding_box) {
            return None;
        }
        let mut piece = VillagePiece::new(
            kind,
            component_type,
            bounding_box,
            direction,
            self.structure_type,
            self.zombie,
        );
        match kind {
            PieceKind::House4Garden => piece.variant = i32::from(random.next_boolean()),
            PieceKind::WoodHut => {
                piece.variant = i32::from(random.next_boolean()) | (random.next_int_bound(3) << 1);
            }
            PieceKind::Field1 => {
                for _ in 0..4 {
                    piece.crops.push(random.next_int_bound(10));
                }
            }
            PieceKind::Field2 => {
                for _ in 0..2 {
                    piece.crops.push(random.next_int_bound(10));
                }
            }
            _ => {}
        }
        Some(piece)
    }

    fn add_road(
        &mut self,
        random: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        direction: Direction,
        component_type: i32,
    ) -> Option<VillagePiece> {
        if component_type > 3 + self.terrain_type
            || (x - self.start_box.min_x).abs() > 112
            || (z - self.start_box.min_z).abs() > 112
        {
            return None;
        }
        let mut bounding_box = None;
        let mut length = 7 * (3 + random.next_int_bound(3));
        while length >= 7 {
            let candidate = component_box(x, y, z, 3, 3, length, direction);
            if !intersects(&self.pieces, candidate) {
                bounding_box = Some(candidate);
                break;
            }
            length -= 7;
        }
        let bounding_box = bounding_box?;
        if bounding_box.min_y <= 10 {
            return None;
        }
        let piece = VillagePiece::new(
            PieceKind::Path,
            component_type,
            bounding_box,
            direction,
            self.structure_type,
            self.zombie,
        );
        self.pieces.push(piece.clone());
        self.pending_roads.push(self.pieces.len() - 1);
        Some(piece)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn topology_is_deterministic_and_immutable_when_cloned() {
        let mut a = JavaRandom::new(123_456);
        let mut b = JavaRandom::new(123_456);
        let first = VillageStructure::create_at(&mut a, 34, -30, 0, 0);
        let second = VillageStructure::create_at(&mut b, 34, -30, 0, 0);
        assert_eq!(first, second);
        assert_eq!(first.valid, first.non_road_count > 2);
        assert_eq!(first.pieces[0].kind, PieceKind::Start);
        assert!(
            first
                .pieces
                .iter()
                .all(|piece| piece.average_ground == -1 && !piece.chest)
        );
        let mut cloned = first.clone();
        cloned.pieces[0].average_ground = 63;
        cloned.pieces[0].bounding_box.offset(0, -1, 0);
        assert_eq!(first.pieces[0].average_ground, -1);
        assert_eq!(first.pieces[0].bounding_box.min_y, 64);
    }

    #[test]
    fn directions_and_component_boxes_cover_negative_coordinates() {
        let north = component_box(-2, 64, -2, 3, 4, 7, Direction::North);
        assert_eq!(north.values(), [-2, 64, -8, 0, 67, -2]);
        let south = component_box(-2, 64, -2, 3, 4, 7, Direction::South);
        assert_eq!(south.values(), [-2, 64, -2, 0, 67, 4]);
        let west = component_box(-2, 64, -2, 3, 4, 7, Direction::West);
        assert_eq!(west.values(), [-8, 64, -2, -2, 67, 0]);
        let east = component_box(-2, 64, -2, 3, 4, 7, Direction::East);
        assert_eq!(east.values(), [-2, 64, -2, 4, 67, 0]);
    }

    #[test]
    fn generation_exercises_weights_variants_and_crops() {
        let mut kinds = BTreeSet::new();
        let mut found_variant = false;
        let mut found_crops = false;
        for seed in 0..48 {
            let mut random = JavaRandom::new(seed);
            let village = VillageStructure::create_at(&mut random, 2, 2, 0, seed as i32 % 4);
            for piece in village.pieces {
                kinds.insert(piece.kind);
                found_variant |= piece.variant != 0;
                found_crops |= !piece.crops.is_empty();
                assert!(piece.crops.iter().all(|&crop| (0..10).contains(&crop)));
            }
        }
        for required in [
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
        ] {
            assert!(kinds.contains(&required), "未生成组件 {required:?}");
        }
        assert!(found_variant && found_crops);
    }

    #[test]
    fn biome_styles_match_village_rules() {
        assert_eq!(structure_type(2), 1);
        assert_eq!(structure_type(35), 2);
        assert_eq!(structure_type(5), 3);
        assert_eq!(structure_type(1), 0);
    }
}
