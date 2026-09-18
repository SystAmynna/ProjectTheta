//! Contenu d'un chunk : une couche de tiles par [`TileLayer`], et son format
//! sérialisé, commun au réseau et aux sauvegardes.
//!
//! # Format
//!
//! Un chunk est la suite de ses couches, dans l'ordre de [`TileLayer::ALL`].
//! Chaque couche est encodée par plages (*run-length encoding*) : une liste de
//! paires `(TileKind, longueur)`, parcourues en row-major, y vers le haut. Le
//! terrain étant fait de grandes étendues uniformes, c'est bien plus court que
//! les [`CHUNK_AREA`] tiles une à une — une couche uniforme tient en une seule
//! plage.
//!
//! La désérialisation refuse une plage vide et une couche dont les plages ne
//! totalisent pas exactement [`CHUNK_AREA`] tiles : des données tronquées ou
//! corrompues ne donnent jamais un chunk incomplet.

use bevy::prelude::*;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::tile::{CHUNK_AREA, TileKind, TileLayer, index_to_local};

/// Une couche de tiles d'un chunk, en row-major, y vers le haut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkLayer(Box<[TileKind; CHUNK_AREA]>);

impl ChunkLayer {
    /// Couche uniformément remplie.
    pub fn filled(kind: TileKind) -> Self {
        Self(Box::new([kind; CHUNK_AREA]))
    }

    /// Couche dont chaque tile est calculée à partir de sa position locale.
    pub fn from_fn(mut tile: impl FnMut(UVec2) -> TileKind) -> Self {
        let mut layer = Self::filled(TileKind::default());
        for (index, slot) in layer.0.iter_mut().enumerate() {
            *slot = tile(index_to_local(index));
        }
        layer
    }

    pub fn get(&self, index: usize) -> TileKind {
        self.0[index]
    }

    /// Remplace une tile. Renvoie `false` si elle avait déjà cette valeur.
    pub fn set(&mut self, index: usize, kind: TileKind) -> bool {
        let changed = self.0[index] != kind;
        self.0[index] = kind;
        changed
    }

    pub fn as_slice(&self) -> &[TileKind] {
        self.0.as_slice()
    }

    /// Plages de tiles identiques consécutives, dans l'ordre des index.
    fn runs(&self) -> Vec<(TileKind, u16)> {
        let mut runs: Vec<(TileKind, u16)> = Vec::new();
        for &kind in self.0.iter() {
            match runs.last_mut() {
                Some((last, len)) if *last == kind => *len += 1,
                _ => runs.push((kind, 1)),
            }
        }
        runs
    }

    /// Inverse de [`runs`](Self::runs). `None` si une plage est vide ou si
    /// leur total n'est pas [`CHUNK_AREA`].
    fn from_runs(runs: &[(TileKind, u16)]) -> Option<Self> {
        let mut layer = Self::filled(TileKind::default());
        let mut index = 0;
        for &(kind, len) in runs {
            let end = index + usize::from(len);
            if len == 0 || end > CHUNK_AREA {
                return None;
            }
            layer.0[index..end].fill(kind);
            index = end;
        }
        (index == CHUNK_AREA).then_some(layer)
    }
}

impl Serialize for ChunkLayer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.runs().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChunkLayer {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let runs = Vec::<(TileKind, u16)>::deserialize(deserializer)?;
        Self::from_runs(&runs).ok_or_else(|| {
            D::Error::invalid_value(
                serde::de::Unexpected::Seq,
                &"des plages non vides totalisant exactement CHUNK_AREA tiles",
            )
        })
    }
}

/// Les tiles d'un chunk : une [`ChunkLayer`] par [`TileLayer`].
///
/// C'est la seule copie des données : le collider (`theta-core`) et le rendu
/// (`theta-render`) en sont dérivés, chacun sur `Changed<ChunkTiles>`. Voir le
/// [module](self) pour le format sérialisé.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkTiles {
    // L'ordre des champs est celui de `TileLayer::ALL`, et fait partie du
    // format sérialisé.
    ground: ChunkLayer,
    object: ChunkLayer,
}

/// Du sol partout, et rien dessus.
impl Default for ChunkTiles {
    fn default() -> Self {
        Self {
            ground: ChunkLayer::filled(TileKind::Floor),
            object: ChunkLayer::filled(TileKind::Empty),
        }
    }
}

impl ChunkTiles {
    /// Chunk dont chaque tile de chaque couche est calculée à partir de sa
    /// position locale.
    pub fn from_fn(mut tile: impl FnMut(TileLayer, UVec2) -> TileKind) -> Self {
        Self {
            ground: ChunkLayer::from_fn(|local| tile(TileLayer::Ground, local)),
            object: ChunkLayer::from_fn(|local| tile(TileLayer::Object, local)),
        }
    }

    pub fn layer(&self, layer: TileLayer) -> &ChunkLayer {
        match layer {
            TileLayer::Ground => &self.ground,
            TileLayer::Object => &self.object,
        }
    }

    pub fn layer_mut(&mut self, layer: TileLayer) -> &mut ChunkLayer {
        match layer {
            TileLayer::Ground => &mut self.ground,
            TileLayer::Object => &mut self.object,
        }
    }

    pub fn get(&self, layer: TileLayer, index: usize) -> TileKind {
        self.layer(layer).get(index)
    }

    /// Remplace une tile. Renvoie `false` si elle avait déjà cette valeur.
    pub fn set(&mut self, layer: TileLayer, index: usize, kind: TileKind) -> bool {
        self.layer_mut(layer).set(index, kind)
    }

    /// La position à cet index est-elle infranchissable : un trou dans le sol,
    /// ou une tile solide sur l'une des couches ?
    pub fn is_solid(&self, index: usize) -> bool {
        TileLayer::ALL
            .iter()
            .any(|&layer| layer.blocks(self.get(layer, index)))
    }

    /// Positions locales infranchissables, toutes couches confondues.
    pub fn solid_tiles(&self) -> impl Iterator<Item = UVec2> + '_ {
        (0..CHUNK_AREA)
            .filter(|&index| self.is_solid(index))
            .map(index_to_local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chunk à la couche d'objets irrégulière : des murs sur un motif en biais.
    fn scattered() -> ChunkTiles {
        ChunkTiles::from_fn(|layer, local| match layer {
            TileLayer::Ground => TileKind::Floor,
            TileLayer::Object if (local.x * 7 + local.y * 3) % 5 == 0 => TileKind::Wall,
            TileLayer::Object => TileKind::Empty,
        })
    }

    #[test]
    fn layers_are_independent() {
        let mut tiles = ChunkTiles::default();
        assert!(tiles.set(TileLayer::Object, 3, TileKind::Wall));
        assert!(!tiles.set(TileLayer::Object, 3, TileKind::Wall));

        assert_eq!(tiles.get(TileLayer::Object, 3), TileKind::Wall);
        assert_eq!(tiles.get(TileLayer::Ground, 3), TileKind::Floor);
        assert!(tiles.is_solid(3));
        assert!(!tiles.is_solid(4));
    }

    #[test]
    fn a_hole_in_the_ground_blocks_the_way() {
        let mut tiles = ChunkTiles::default();
        tiles.set(TileLayer::Ground, 8, TileKind::Empty);

        assert!(tiles.is_solid(8));
        assert!(!tiles.is_solid(9));
        assert_eq!(tiles.solid_tiles().count(), 1);
    }

    #[test]
    fn round_trips_through_serde() {
        for tiles in [ChunkTiles::default(), scattered()] {
            let bytes = postcard::to_allocvec(&tiles).unwrap();
            assert_eq!(postcard::from_bytes::<ChunkTiles>(&bytes).unwrap(), tiles);
        }
    }

    /// Octets figés d'un chunk simple : s'ils changent, les sauvegardes et le
    /// protocole existants ne sont plus lisibles. C'est voulu seulement quand
    /// on change le format — et alors `PROTOCOL_ID` aussi.
    #[test]
    fn serialized_format_is_stable() {
        let mut tiles = ChunkTiles::default();
        tiles.set(TileLayer::Object, 0, TileKind::Wall);

        let bytes = postcard::to_allocvec(&tiles).unwrap();
        // Sol : 1 plage (Floor, 1024). Objets : 2 plages (Wall, 1), (Empty, 1023).
        // Longueurs et entiers en varint : 1024 = [0x80, 0x08], 1023 = [0xFF, 0x07].
        assert_eq!(
            bytes,
            [1, 1, 0x80, 0x08, 2, 2, 1, 0, 0xFF, 0x07],
            "{bytes:02X?}"
        );
    }

    #[test]
    fn run_length_encoding_shrinks_the_chunk() {
        let raw = postcard::to_allocvec(&vec![TileKind::Floor; 2 * CHUNK_AREA]).unwrap();
        let uniform = postcard::to_allocvec(&ChunkTiles::default()).unwrap();
        let busy = postcard::to_allocvec(&scattered()).unwrap();

        assert!(uniform.len() < 10, "{} octets", uniform.len());
        assert!(
            busy.len() < raw.len(),
            "{} octets contre {}",
            busy.len(),
            raw.len()
        );
    }

    #[test]
    fn incomplete_or_overflowing_layers_are_rejected() {
        let layer = |runs: &[(TileKind, u16)]| postcard::to_allocvec(runs).unwrap();
        let area = CHUNK_AREA as u16;

        for runs in [
            layer(&[(TileKind::Floor, area - 1)]),
            layer(&[(TileKind::Floor, area), (TileKind::Wall, 1)]),
            layer(&[(TileKind::Floor, 0), (TileKind::Floor, area)]),
            layer(&[]),
        ] {
            assert!(
                postcard::from_bytes::<ChunkLayer>(&runs).is_err(),
                "{runs:02X?}"
            );
        }
        assert!(postcard::from_bytes::<ChunkLayer>(&layer(&[(TileKind::Wall, area)])).is_ok());
    }

    #[test]
    fn unknown_tile_kind_is_rejected() {
        // Variante 9 : n'existe pas.
        let bytes = [1, 9, 0x80, 0x08];
        assert!(postcard::from_bytes::<ChunkLayer>(&bytes).is_err());
    }
}
