use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Côté d'une tile, en pixels.
pub const TILE_SIZE: f32 = 64.0;

/// Côté d'un chunk, en tiles.
pub const CHUNK_SIZE: u32 = 32;

/// Nombre de tiles dans un chunk.
pub const CHUNK_AREA: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// Côté d'un chunk, en pixels.
pub const CHUNK_WORLD_SIZE: f32 = TILE_SIZE * CHUNK_SIZE as f32;

/// Nature d'une tile pour la simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TileKind {
    /// Sol praticable.
    #[default]
    Floor,
    /// Mur infranchissable.
    Wall,
}

impl TileKind {
    /// La tile bloque-t-elle le passage ?
    pub const fn is_solid(self) -> bool {
        matches!(self, Self::Wall)
    }
}

/// Coordonnée d'une tile dans le monde, en tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoord(pub IVec2);

impl TileCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self(IVec2::new(x, y))
    }

    /// Tile qui contient un point du monde.
    pub fn from_world(position: Vec2) -> Self {
        Self((position / TILE_SIZE).floor().as_ivec2())
    }

    /// Centre de la tile, en pixels.
    pub fn center(self) -> Vec2 {
        (self.0.as_vec2() + 0.5) * TILE_SIZE
    }

    /// Chunk qui contient la tile.
    pub fn chunk(self) -> ChunkCoord {
        ChunkCoord(self.0.div_euclid(IVec2::splat(CHUNK_SIZE as i32)))
    }

    /// Position de la tile à l'intérieur de son chunk, de `0` à `CHUNK_SIZE - 1`.
    pub fn local(self) -> UVec2 {
        self.0
            .rem_euclid(IVec2::splat(CHUNK_SIZE as i32))
            .as_uvec2()
    }

    /// Index de la tile dans [`ChunkTiles`](super::ChunkTiles).
    pub fn local_index(self) -> usize {
        local_to_index(self.local())
    }
}

/// Coordonnée d'un chunk, en chunks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord(pub IVec2);

impl ChunkCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self(IVec2::new(x, y))
    }

    /// Chunk qui contient un point du monde.
    pub fn from_world(position: Vec2) -> Self {
        TileCoord::from_world(position).chunk()
    }

    /// Centre du chunk, en pixels : c'est là qu'est posée son entité.
    pub fn center(self) -> Vec2 {
        (self.0.as_vec2() + 0.5) * CHUNK_WORLD_SIZE
    }

    /// Tile du chunk à une position locale donnée.
    pub fn tile(self, local: UVec2) -> TileCoord {
        TileCoord(self.0 * CHUNK_SIZE as i32 + local.as_ivec2())
    }

    /// Distance de Chebyshev, en chunks : `1` pour les huit voisins.
    pub fn distance(self, other: Self) -> u32 {
        (self.0 - other.0).abs().max_element() as u32
    }
}

/// Index row-major, y vers le haut, d'une position locale : l'ordre attendu par
/// `TilemapChunkTileData` côté rendu.
pub const fn local_to_index(local: UVec2) -> usize {
    (local.y * CHUNK_SIZE + local.x) as usize
}

/// Inverse de [`local_to_index`].
pub const fn index_to_local(index: usize) -> UVec2 {
    UVec2::new(index as u32 % CHUNK_SIZE, index as u32 / CHUNK_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_tile_handles_negative_coordinates() {
        assert_eq!(
            TileCoord::from_world(Vec2::new(0.0, TILE_SIZE - 0.1)),
            TileCoord::new(0, 0)
        );
        assert_eq!(
            TileCoord::from_world(Vec2::new(-0.1, TILE_SIZE)),
            TileCoord::new(-1, 1)
        );
    }

    #[test]
    fn tile_to_chunk_handles_negative_coordinates() {
        let size = CHUNK_SIZE as i32;

        assert_eq!(TileCoord::new(0, size - 1).chunk(), ChunkCoord::new(0, 0));
        assert_eq!(TileCoord::new(-1, size).chunk(), ChunkCoord::new(-1, 1));
        assert_eq!(TileCoord::new(-1, 0).local(), UVec2::new(CHUNK_SIZE - 1, 0));
    }

    #[test]
    fn chunk_and_local_round_trip() {
        for coord in [
            TileCoord::new(5, -7),
            TileCoord::new(-33, 64),
            TileCoord::new(0, 0),
        ] {
            assert_eq!(coord.chunk().tile(coord.local()), coord);
            assert_eq!(index_to_local(coord.local_index()), coord.local());
        }
    }

    #[test]
    fn chunk_center_is_the_middle_of_its_tiles() {
        let chunk = ChunkCoord::new(-2, 3);
        let first = chunk.tile(UVec2::ZERO).center();
        let last = chunk.tile(UVec2::splat(CHUNK_SIZE - 1)).center();

        assert_eq!((first + last) / 2.0, chunk.center());
        assert_eq!(ChunkCoord::from_world(chunk.center()), chunk);
    }

    #[test]
    fn chunk_distance_is_chebyshev() {
        let origin = ChunkCoord::new(0, 0);
        assert_eq!(origin.distance(ChunkCoord::new(1, 1)), 1);
        assert_eq!(origin.distance(ChunkCoord::new(-2, 1)), 2);
    }
}
