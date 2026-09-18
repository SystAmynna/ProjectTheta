//! Génération du monde, **côté serveur uniquement**.
//!
//! Le client ne dépend pas de cette crate : il ne connaît du terrain que les
//! chunks que le serveur lui envoie. Le serveur, lui, génère chaque chunk à la
//! demande, au moment où un joueur s'en approche.
//!
//! Chaque tile est une fonction pure de `(graine, coordonnée globale)` : un chunk
//! se calcule seul, sans connaître ses voisins, et se raccorde pourtant sans
//! couture avec eux. C'est aussi ce qui permet au serveur d'oublier un chunk
//! jamais modifié : le régénérer redonne exactement le même.

use bevy::prelude::*;
use theta_core::terrain::TILE_SIZE;
use theta_core::{ChunkCoord, ChunkTiles, SPAWN_RADIUS, TileCoord, TileKind};

/// Taille des formations de la première octave, en tiles.
const FEATURE_SIZE: f32 = 16.0;

/// Nombre d'octaves du bruit : chacune double la fréquence et divise
/// l'amplitude par deux, pour ajouter du détail aux bords des murs.
const OCTAVES: u32 = 3;

/// Au-dessus de ce seuil, le bruit donne un mur.
///
/// Réglé pour environ un quart de murs : des îlots et des parois séparés par
/// de larges passages, où la hitbox du joueur (une tile de diamètre) circule
/// sans peine.
const WALL_THRESHOLD: f32 = 0.6;

/// Rayon du disque de sol garanti autour de l'origine, en pixels : le cercle des
/// points d'apparition, plus une marge pour la hitbox du joueur.
const SPAWN_CLEARING: f32 = SPAWN_RADIUS + 3.0 * TILE_SIZE;

/// Générateur de terrain, déterminé par sa seule graine.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldGenerator {
    seed: u64,
}

impl WorldGenerator {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Tiles d'un chunk.
    pub fn generate_chunk(&self, chunk: ChunkCoord) -> ChunkTiles {
        ChunkTiles::from_fn(|local| self.tile(chunk.tile(local)))
    }

    /// Nature d'une tile, indépendamment du chunk qui la contient.
    pub fn tile(&self, coord: TileCoord) -> TileKind {
        if coord.center().length() < SPAWN_CLEARING {
            return TileKind::Floor;
        }

        if self.fbm(coord.0.as_vec2() / FEATURE_SIZE) > WALL_THRESHOLD {
            TileKind::Wall
        } else {
            TileKind::Floor
        }
    }

    /// Somme d'octaves de bruit de valeur, normalisée dans `[0, 1]`.
    fn fbm(&self, point: Vec2) -> f32 {
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut frequency = 1.0;

        for octave in 0..OCTAVES {
            sum += amplitude * self.value_noise(point * frequency, octave);
            total += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }

        sum / total
    }

    /// Bruit de valeur : une valeur pseudo-aléatoire par nœud entier du réseau,
    /// interpolée en douceur (smoothstep) entre les quatre nœuds voisins.
    fn value_noise(&self, point: Vec2, octave: u32) -> f32 {
        let cell = point.floor();
        let origin = cell.as_ivec2();
        let t = point - cell;
        let t = t * t * (3.0 - 2.0 * t);

        let corner = |dx, dy| self.lattice(origin + IVec2::new(dx, dy), octave);
        let bottom = corner(0, 0).lerp(corner(1, 0), t.x);
        let top = corner(0, 1).lerp(corner(1, 1), t.x);
        bottom.lerp(top, t.y)
    }

    /// Valeur d'un nœud du réseau, dans `[0, 1)`.
    fn lattice(&self, node: IVec2, octave: u32) -> f32 {
        let key = (node.x as u32 as u64) | ((node.y as u32 as u64) << 32);
        let hash = splitmix64(self.seed ^ splitmix64(key ^ splitmix64(octave as u64)));

        // Les 24 bits de poids fort tiennent exactement dans la mantisse d'un f32.
        (hash >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Finaliseur de SplitMix64 : mélange complètement les bits d'un entier.
///
/// Écrit ici plutôt qu'emprunté à `rand`, dont les algorithmes ne sont pas
/// garantis stables d'une version à l'autre : une même graine doit redonner le
/// même monde après une mise à jour.
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use theta_core::spawn_point;
    use theta_core::terrain::{CHUNK_AREA, CHUNK_SIZE, index_to_local};

    use super::*;

    const SEED: u64 = 42;

    #[test]
    fn same_seed_same_chunk() {
        let chunk = ChunkCoord::new(3, -7);
        assert_eq!(
            WorldGenerator::new(SEED).generate_chunk(chunk),
            WorldGenerator::new(SEED).generate_chunk(chunk)
        );
    }

    #[test]
    fn different_seeds_give_different_worlds() {
        let chunk = ChunkCoord::new(3, -7);
        assert_ne!(
            WorldGenerator::new(SEED).generate_chunk(chunk),
            WorldGenerator::new(SEED + 1).generate_chunk(chunk)
        );
    }

    /// Empreinte figée d'un chunk : si elle change, la même graine ne redonne
    /// plus le même monde. C'est voulu seulement quand on modifie le générateur.
    #[test]
    fn generation_is_stable_across_builds() {
        let tiles = WorldGenerator::new(SEED).generate_chunk(ChunkCoord::new(5, 5));
        let fingerprint = tiles
            .as_slice()
            .iter()
            .fold(0u64, |hash, &kind| splitmix64(hash ^ kind as u64));

        assert_eq!(
            fingerprint, 0x4DAD_EDB4_A469_9E52,
            "empreinte : {fingerprint:#X}"
        );
    }

    #[test]
    fn chunks_match_the_global_tile_function() {
        let generator = WorldGenerator::new(SEED);

        for chunk in [
            ChunkCoord::new(0, 0),
            ChunkCoord::new(-1, 0),
            ChunkCoord::new(2, -3),
        ] {
            let tiles = generator.generate_chunk(chunk);
            for index in 0..CHUNK_AREA {
                let tile = chunk.tile(index_to_local(index));
                assert_eq!(tiles.get(index), generator.tile(tile), "tile {:?}", tile.0);
            }
        }
    }

    #[test]
    fn spawn_ring_is_always_floor() {
        for seed in 0..16 {
            let generator = WorldGenerator::new(seed);
            for index in 0..8 {
                let spawn = spawn_point(index);
                // Toute la hitbox du joueur, pas seulement son centre.
                for offset in [Vec2::ZERO, Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                    let point = spawn + offset * 32.0;
                    let tile = TileCoord::from_world(point);
                    assert_eq!(
                        generator.tile(tile),
                        TileKind::Floor,
                        "graine {seed}, {point:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn world_has_walls_and_room_to_walk() {
        let generator = WorldGenerator::new(SEED);
        let chunks = (-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y)));

        let (mut walls, mut total) = (0, 0);
        for chunk in chunks {
            let tiles = generator.generate_chunk(chunk);
            walls += tiles.solid_tiles().count();
            total += tiles.as_slice().len();
        }

        let ratio = walls as f32 / total as f32;
        assert!((0.1..0.45).contains(&ratio), "proportion de murs : {ratio}");
    }

    /// Aperçu ASCII d'une zone, pour régler le générateur à l'œil :
    /// `cargo test -p theta-worldgen preview -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn preview() {
        let generator = WorldGenerator::new(SEED);
        let size = CHUNK_SIZE as i32 * 2;

        for y in (-size..size).rev() {
            let row: String = (-size..size)
                .map(|x| match generator.tile(TileCoord::new(x, y)) {
                    TileKind::Wall => '#',
                    TileKind::Floor => '.',
                })
                .collect();
            println!("{row}");
        }
    }
}
