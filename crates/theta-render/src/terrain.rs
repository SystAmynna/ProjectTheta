//! Rendu du terrain : un `TilemapChunk` par chunk.
//!
//! `TilemapChunk` (intégré à Bevy) dessine un chunk entier en un seul mesh et un
//! seul draw call ; les indices de tiles vivent dans une petite texture, seule
//! ré-envoyée au GPU quand le chunk change. Ce module ne fait qu'habiller les
//! chunks que `theta-core` crée : il ne décide d'aucune tile.

use bevy::prelude::*;
use bevy::sprite_render::{AlphaMode2d, TileData, TilemapChunk, TilemapChunkTileData};
use theta_core::terrain::{CHUNK_SIZE, TILE_SIZE, index_to_local};
use theta_core::{ChunkCoord, ChunkTiles, TerrainChunk, TileCoord, TileKind};

use crate::GameAssets;

/// Profondeur du terrain : sous les joueurs, dessinés en `z = 0`.
const TERRAIN_Z: f32 = -10.0;

pub(crate) struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach_chunk_render, update_chunk_render));
    }
}

/// Couches du tileset (`assets/tiles.png`), de haut en bas.
pub struct TilesetIndex;

impl TilesetIndex {
    pub const FLOOR: u16 = 0;
    pub const FLOOR_ALT: u16 = 1;
    pub const WALL: u16 = 2;

    /// Nombre de couches du tileset.
    pub const COUNT: u32 = 3;

    /// Couche à afficher pour une tile.
    ///
    /// La variante de sol est tirée de la position : purement visuelle, elle
    /// n'existe pas pour la simulation et reste la même d'une image à l'autre.
    pub fn of(kind: TileKind, coord: TileCoord) -> u16 {
        match kind {
            TileKind::Wall => Self::WALL,
            TileKind::Floor => {
                let hash = (coord.0.x as u32).wrapping_mul(0x9E37_79B1)
                    ^ (coord.0.y as u32).wrapping_mul(0x85EB_CA77);
                if hash >> 29 == 0 {
                    Self::FLOOR_ALT
                } else {
                    Self::FLOOR
                }
            }
        }
    }
}

/// Entité enfant qui porte le rendu d'un chunk.
///
/// Le rendu vit sur un enfant plutôt que sur le chunk lui-même : le chunk est un
/// corps physique dont `Transform` est réécrit depuis `Position`, et l'enfant
/// garde ainsi sa propre profondeur.
#[derive(Component, Debug)]
struct ChunkRender(Entity);

/// Donne un rendu à chaque chunk qui n'en a pas encore.
fn attach_chunk_render(
    mut commands: Commands,
    assets: Res<GameAssets>,
    chunks: Query<(Entity, &TerrainChunk, &ChunkTiles), Without<ChunkRender>>,
) {
    for (entity, &TerrainChunk(coord), tiles) in &chunks {
        // `theta-core` ne connaît pas la visibilité ; sans elle sur le parent,
        // l'enfant affiché hériterait d'une hiérarchie incohérente (B0004).
        commands.entity(entity).insert(Visibility::default());

        let render = commands
            .spawn((
                Name::new("Chunk render"),
                TilemapChunk {
                    chunk_size: UVec2::splat(CHUNK_SIZE),
                    tile_display_size: UVec2::splat(TILE_SIZE as u32),
                    tileset: assets.tileset.clone(),
                    // Aucune tile n'est transparente : pas de tri ni de
                    // mélange à payer.
                    alpha_mode: AlphaMode2d::Opaque,
                },
                TilemapChunkTileData(tile_data(coord, tiles)),
                Transform::from_xyz(0.0, 0.0, TERRAIN_Z),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(ChunkRender(render));
    }
}

/// Répercute sur le rendu les tiles modifiées d'un chunk.
fn update_chunk_render(
    chunks: Query<(&TerrainChunk, &ChunkTiles, &ChunkRender), Changed<ChunkTiles>>,
    mut renders: Query<&mut TilemapChunkTileData>,
) {
    for (&TerrainChunk(coord), tiles, render) in &chunks {
        if let Ok(mut data) = renders.get_mut(render.0) {
            data.0 = tile_data(coord, tiles);
        }
    }
}

/// Indices du tileset d'un chunk, dans l'ordre de `TilemapChunkTileData` :
/// row-major, y vers le haut — le même que [`ChunkTiles`].
fn tile_data(coord: ChunkCoord, tiles: &ChunkTiles) -> Vec<Option<TileData>> {
    tiles
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, &kind)| {
            let tile = coord.tile(index_to_local(index));
            Some(TileData::from_tileset_index(TilesetIndex::of(kind, tile)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use theta_core::terrain::CHUNK_AREA;

    use super::*;
    use crate::asset_root;

    #[test]
    fn tileset_has_one_square_layer_per_index() {
        let path = asset_root().join(crate::assets::TILESET_PATH);
        let bytes = std::fs::read(&path).unwrap();
        // En-tête PNG : largeur et hauteur, en big-endian, dans le bloc IHDR.
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        assert_eq!(
            height,
            width * TilesetIndex::COUNT,
            "{path:?} : {width} × {height}"
        );
    }

    #[test]
    fn tileset_index_depends_only_on_kind_and_position() {
        let coord = TileCoord::new(-12, 40);
        assert_eq!(TilesetIndex::of(TileKind::Wall, coord), TilesetIndex::WALL);
        let floor = TilesetIndex::of(TileKind::Floor, coord);
        assert!([TilesetIndex::FLOOR, TilesetIndex::FLOOR_ALT].contains(&floor));
        assert_eq!(TilesetIndex::of(TileKind::Floor, coord), floor);
        assert!(floor < TilesetIndex::COUNT as u16);
    }

    #[test]
    fn tile_data_covers_the_whole_chunk() {
        let data = tile_data(ChunkCoord::new(0, 0), &ChunkTiles::filled(TileKind::Wall));
        assert_eq!(data.len(), CHUNK_AREA);
    }
}
