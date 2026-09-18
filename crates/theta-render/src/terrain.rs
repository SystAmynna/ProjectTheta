//! Rendu du terrain : un `TilemapChunk` par couche de chaque chunk.
//!
//! `TilemapChunk` (intégré à Bevy) dessine une couche entière en un seul mesh et
//! un seul draw call ; les indices de tiles vivent dans une petite texture, seule
//! ré-envoyée au GPU quand le chunk change. Ce module ne fait qu'habiller les
//! chunks que `theta-core` crée : il ne décide d'aucune tile.

use bevy::prelude::*;
use bevy::sprite_render::{AlphaMode2d, TileData, TilemapChunk, TilemapChunkTileData};
use theta_core::terrain::{CHUNK_SIZE, TILE_SIZE, index_to_local};
use theta_core::{ChunkCoord, ChunkTiles, TerrainChunk, TileCoord, TileKind, TileLayer};

use crate::GameAssets;

/// Profondeur de la couche de sol, sous les joueurs dessinés en `z = 0`. Chaque
/// couche suivante est dessinée juste au-dessus de la précédente.
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

    /// Couche à afficher pour une tile, ou `None` pour une tile vide.
    ///
    /// La variante de sol est tirée de la position : purement visuelle, elle
    /// n'existe pas pour la simulation et reste la même d'une image à l'autre.
    pub fn of(kind: TileKind, coord: TileCoord) -> Option<u16> {
        match kind {
            TileKind::Empty => None,
            TileKind::Wall => Some(Self::WALL),
            TileKind::Floor => {
                let hash = (coord.0.x as u32).wrapping_mul(0x9E37_79B1)
                    ^ (coord.0.y as u32).wrapping_mul(0x85EB_CA77);
                Some(if hash >> 29 == 0 {
                    Self::FLOOR_ALT
                } else {
                    Self::FLOOR
                })
            }
        }
    }
}

/// Entités enfants qui portent le rendu d'un chunk, une par couche, dans
/// l'ordre de [`TileLayer::ALL`].
///
/// Le rendu vit sur des enfants plutôt que sur le chunk lui-même : le chunk est
/// un corps physique dont `Transform` est réécrit depuis `Position`, et chaque
/// enfant garde ainsi sa propre profondeur.
#[derive(Component, Debug)]
struct ChunkRender([Entity; TileLayer::ALL.len()]);

/// Donne un rendu à chaque chunk qui n'en a pas encore.
fn attach_chunk_render(
    mut commands: Commands,
    assets: Res<GameAssets>,
    chunks: Query<(Entity, &TerrainChunk, &ChunkTiles), Without<ChunkRender>>,
) {
    for (entity, &TerrainChunk(coord), tiles) in &chunks {
        // `theta-core` ne connaît pas la visibilité ; sans elle sur le parent,
        // les enfants affichés hériteraient d'une hiérarchie incohérente (B0004).
        commands.entity(entity).insert(Visibility::default());

        let renders = TileLayer::ALL.map(|layer| {
            commands
                .spawn((
                    Name::new(format!("Chunk render {layer:?}")),
                    TilemapChunk {
                        chunk_size: UVec2::splat(CHUNK_SIZE),
                        tile_display_size: UVec2::splat(TILE_SIZE as u32),
                        tileset: assets.tileset.clone(),
                        // Aucune tile n'est translucide, et les tiles vides
                        // sont écartées par le shader : pas de tri ni de
                        // mélange à payer, même pour les couches du dessus.
                        alpha_mode: AlphaMode2d::Opaque,
                    },
                    TilemapChunkTileData(tile_data(coord, tiles, layer)),
                    Transform::from_xyz(0.0, 0.0, TERRAIN_Z + layer as u8 as f32),
                    ChildOf(entity),
                ))
                .id()
        });
        commands.entity(entity).insert(ChunkRender(renders));
    }
}

/// Répercute sur le rendu les tiles modifiées d'un chunk.
fn update_chunk_render(
    chunks: Query<(&TerrainChunk, &ChunkTiles, &ChunkRender), Changed<ChunkTiles>>,
    mut renders: Query<&mut TilemapChunkTileData>,
) {
    for (&TerrainChunk(coord), tiles, render) in &chunks {
        for (layer, &entity) in TileLayer::ALL.iter().zip(&render.0) {
            if let Ok(mut data) = renders.get_mut(entity) {
                data.0 = tile_data(coord, tiles, *layer);
            }
        }
    }
}

/// Indices du tileset d'une couche d'un chunk, dans l'ordre de
/// `TilemapChunkTileData` : row-major, y vers le haut — le même que
/// [`ChunkTiles`]. Les tiles vides valent `None` et ne sont pas dessinées : un
/// trou dans le sol laisse voir le vide ([`VOID_COLOR`](crate::VOID_COLOR)).
fn tile_data(coord: ChunkCoord, tiles: &ChunkTiles, layer: TileLayer) -> Vec<Option<TileData>> {
    tiles
        .layer(layer)
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, &kind)| {
            let tile = coord.tile(index_to_local(index));
            TilesetIndex::of(kind, tile).map(TileData::from_tileset_index)
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
        assert_eq!(TilesetIndex::of(TileKind::Empty, coord), None);
        assert_eq!(
            TilesetIndex::of(TileKind::Wall, coord),
            Some(TilesetIndex::WALL)
        );
        let floor = TilesetIndex::of(TileKind::Floor, coord).unwrap();
        assert!([TilesetIndex::FLOOR, TilesetIndex::FLOOR_ALT].contains(&floor));
        assert_eq!(TilesetIndex::of(TileKind::Floor, coord), Some(floor));
        assert!(floor < TilesetIndex::COUNT as u16);
    }

    #[test]
    fn tile_data_covers_the_whole_chunk_and_skips_empty_tiles() {
        let mut tiles = ChunkTiles::default();
        tiles.set(TileLayer::Object, 5, TileKind::Wall);
        let chunk = ChunkCoord::new(0, 0);

        let ground = tile_data(chunk, &tiles, TileLayer::Ground);
        assert_eq!(ground.len(), CHUNK_AREA);
        assert!(ground.iter().all(Option::is_some));

        let mut holed = ChunkTiles::default();
        holed.set(TileLayer::Ground, 3, TileKind::Empty);
        let ground = tile_data(chunk, &holed, TileLayer::Ground);
        assert!(ground[3].is_none(), "un trou n'est pas dessiné");

        let object = tile_data(chunk, &tiles, TileLayer::Object);
        assert_eq!(object.len(), CHUNK_AREA);
        assert_eq!(object.iter().filter(|tile| tile.is_some()).count(), 1);
        assert!(object[5].is_some());
    }
}
