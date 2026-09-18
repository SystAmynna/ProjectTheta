//! Génération du monde, **côté serveur uniquement**.
//!
//! Le client ne dépend pas de cette crate : il ne connaît du terrain que les
//! chunks que le serveur lui envoie. Le serveur, lui, génère chaque chunk à la
//! demande, au moment où un joueur s'en approche.
//!
//! Deux bruits indépendants, tirés de la même graine, façonnent le terrain : l'un
//! pose les murs, l'autre creuse des trous dans le sol, jamais sous un mur. Le
//! disque des points d'apparition reste toujours praticable.
//!
//! Chaque tile est une fonction pure de `(graine, coordonnée globale)` : un chunk
//! se calcule seul, sans connaître ses voisins, et se raccorde pourtant sans
//! couture avec eux. C'est aussi ce qui permet au serveur d'oublier un chunk
//! jamais modifié : le régénérer redonne exactement le même.

use bevy::prelude::*;
use theta_core::terrain::TILE_SIZE;
use theta_core::{ChunkCoord, ChunkTiles, SPAWN_RADIUS, TileCoord, TileKind, TileLayer};

/// Taille des massifs de murs à la première octave, en tiles.
const WALL_FEATURE_SIZE: f32 = 16.0;

/// Taille des trous à la première octave, en tiles : plus petits que les
/// massifs de murs.
const HOLE_FEATURE_SIZE: f32 = 10.0;

/// Nombre d'octaves du bruit : chacune double la fréquence et divise
/// l'amplitude par deux, pour ajouter du détail aux bords des formations.
const OCTAVES: u32 = 3;

/// Au-dessus de ce seuil, le bruit donne un mur.
///
/// Réglé pour environ un quart de murs : des îlots et des parois séparés par
/// de larges passages, où la hitbox du joueur (une tile de diamètre) circule
/// sans peine.
const WALL_THRESHOLD: f32 = 0.6;

/// Au-dessus de ce seuil, le bruit des trous retire le sol.
///
/// Plus haut que celui des murs : des trous épars, en plus des murs, qui
/// n'obstruent pas les passages.
const HOLE_THRESHOLD: f32 = 0.68;

/// Sel de la graine des trous : leur bruit est indépendant de celui des murs,
/// qui garde la graine telle quelle.
const HOLE_SALT: u64 = 0x486F_6C65;

/// Rayon du disque garanti praticable (ni mur, ni trou) autour de l'origine, en
/// pixels : le cercle des points d'apparition, plus une marge pour la hitbox du
/// joueur.
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
        ChunkTiles::from_fn(|layer, local| self.tile(layer, chunk.tile(local)))
    }

    /// Nature d'une tile d'une couche, indépendamment du chunk qui la contient.
    ///
    /// Les murs sont posés sur la couche d'objets. Le sol est partout, sauf là
    /// où le bruit des trous le retire ; un mur, prioritaire, repose toujours
    /// sur du sol.
    pub fn tile(&self, layer: TileLayer, coord: TileCoord) -> TileKind {
        match layer {
            TileLayer::Ground if self.is_hole(coord) => TileKind::Empty,
            TileLayer::Ground => TileKind::Floor,
            TileLayer::Object if self.is_wall(coord) => TileKind::Wall,
            TileLayer::Object => TileKind::Empty,
        }
    }

    fn is_wall(&self, coord: TileCoord) -> bool {
        !in_spawn_clearing(coord)
            && fbm(self.seed, coord.0.as_vec2() / WALL_FEATURE_SIZE) > WALL_THRESHOLD
    }

    fn is_hole(&self, coord: TileCoord) -> bool {
        !in_spawn_clearing(coord)
            && !self.is_wall(coord)
            && fbm(
                splitmix64(self.seed ^ HOLE_SALT),
                coord.0.as_vec2() / HOLE_FEATURE_SIZE,
            ) > HOLE_THRESHOLD
    }
}

/// La tile est-elle dans le disque gardé praticable autour de l'origine ?
fn in_spawn_clearing(coord: TileCoord) -> bool {
    coord.center().length() < SPAWN_CLEARING
}

/// Somme d'octaves de bruit de valeur, normalisée dans `[0, 1]`.
fn fbm(seed: u64, point: Vec2) -> f32 {
    let mut sum = 0.0;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    let mut frequency = 1.0;

    for octave in 0..OCTAVES {
        sum += amplitude * value_noise(seed, point * frequency, octave);
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    sum / total
}

/// Bruit de valeur : une valeur pseudo-aléatoire par nœud entier du réseau,
/// interpolée en douceur (smoothstep) entre les quatre nœuds voisins.
fn value_noise(seed: u64, point: Vec2, octave: u32) -> f32 {
    let cell = point.floor();
    let origin = cell.as_ivec2();
    let t = point - cell;
    let t = t * t * (3.0 - 2.0 * t);

    let corner = |dx, dy| lattice(seed, origin + IVec2::new(dx, dy), octave);
    let bottom = corner(0, 0).lerp(corner(1, 0), t.x);
    let top = corner(0, 1).lerp(corner(1, 1), t.x);
    bottom.lerp(top, t.y)
}

/// Valeur d'un nœud du réseau, dans `[0, 1)`.
fn lattice(seed: u64, node: IVec2, octave: u32) -> f32 {
    let key = (node.x as u32 as u64) | ((node.y as u32 as u64) << 32);
    let hash = splitmix64(seed ^ splitmix64(key ^ splitmix64(octave as u64)));

    // Les 24 bits de poids fort tiennent exactement dans la mantisse d'un f32.
    (hash >> 40) as f32 / (1u64 << 24) as f32
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

    const HOLES_FINGERPRINT: u64 = 0x927D_AA8D_EF07_C392;

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

    /// Empreinte d'une couche d'un chunk : une tile de cette nature, ou non.
    fn fingerprint(tiles: &ChunkTiles, layer: TileLayer, kind: TileKind) -> u64 {
        (0..CHUNK_AREA).fold(0u64, |hash, index| {
            splitmix64(hash ^ (tiles.get(layer, index) == kind) as u64)
        })
    }

    /// Empreintes figées des murs et des trous d'un chunk : si elles changent,
    /// la même graine ne redonne plus le même monde. C'est voulu seulement
    /// quand on modifie le générateur.
    #[test]
    fn generation_is_stable_across_builds() {
        let tiles = WorldGenerator::new(SEED).generate_chunk(ChunkCoord::new(5, 5));

        let walls = fingerprint(&tiles, TileLayer::Object, TileKind::Wall);
        assert_eq!(walls, 0x4DAD_EDB4_A469_9E52, "murs : {walls:#X}");

        let holes = fingerprint(&tiles, TileLayer::Ground, TileKind::Empty);
        assert_eq!(holes, HOLES_FINGERPRINT, "trous : {holes:#X}");
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
                for layer in TileLayer::ALL {
                    assert_eq!(
                        tiles.get(layer, index),
                        generator.tile(layer, tile),
                        "tile {:?}, {layer:?}",
                        tile.0
                    );
                }
            }
        }
    }

    #[test]
    fn spawn_ring_is_always_walkable() {
        for seed in 0..16 {
            let generator = WorldGenerator::new(seed);
            for index in 0..8 {
                let spawn = spawn_point(index);
                // Toute la hitbox du joueur, pas seulement son centre.
                for offset in [Vec2::ZERO, Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                    let point = spawn + offset * 32.0;
                    let tile = TileCoord::from_world(point);
                    assert_eq!(
                        generator.tile(TileLayer::Object, tile),
                        TileKind::Empty,
                        "mur : graine {seed}, {point:?}"
                    );
                    assert_eq!(
                        generator.tile(TileLayer::Ground, tile),
                        TileKind::Floor,
                        "trou : graine {seed}, {point:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn world_has_walls_holes_and_room_to_walk() {
        let generator = WorldGenerator::new(SEED);
        let chunks = (-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y)));

        let (mut walls, mut holes, mut total) = (0, 0, 0);
        for chunk in chunks {
            let tiles = generator.generate_chunk(chunk);
            for index in 0..CHUNK_AREA {
                let wall = tiles.get(TileLayer::Object, index) == TileKind::Wall;
                let hole = tiles.get(TileLayer::Ground, index) == TileKind::Empty;
                assert!(!(wall && hole), "un mur repose toujours sur du sol");
                walls += wall as usize;
                holes += hole as usize;
            }
            total += CHUNK_AREA;
        }

        let walls = walls as f32 / total as f32;
        let holes = holes as f32 / total as f32;
        println!("murs {walls:.3}, trous {holes:.3}");
        assert!((0.1..0.35).contains(&walls), "proportion de murs : {walls}");
        assert!(
            (0.02..0.15).contains(&holes),
            "proportion de trous : {holes}"
        );
        assert!(
            walls + holes < 0.45,
            "trop peu de place : {}",
            walls + holes
        );
    }

    /// Taille sérialisée des chunks générés, telle qu'elle part sur le réseau
    /// (lightyear sérialise avec postcard) ou en sauvegarde.
    #[test]
    fn generated_chunks_compress_well() {
        let generator = WorldGenerator::new(SEED);
        let sizes: Vec<usize> = (-4..4)
            .flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y)))
            .map(|chunk| {
                postcard::to_allocvec(&generator.generate_chunk(chunk))
                    .unwrap()
                    .len()
            })
            .collect();

        // Sans compression : un octet par tile et par couche.
        let raw = TileLayer::ALL.len() * CHUNK_AREA;
        let largest = *sizes.iter().max().unwrap();
        let mean = sizes.iter().sum::<usize>() / sizes.len();
        println!("brut {raw} o, moyenne {mean} o, pire {largest} o");
        assert!(largest < raw / 4, "pire chunk : {largest} octets sur {raw}");
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
                .map(|x| {
                    let tile = TileCoord::new(x, y);
                    if generator.tile(TileLayer::Object, tile) == TileKind::Wall {
                        '#'
                    } else if generator.tile(TileLayer::Ground, tile) == TileKind::Empty {
                        ' '
                    } else {
                        '.'
                    }
                })
                .collect();
            println!("{row}");
        }
    }
}
