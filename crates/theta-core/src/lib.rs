use std::time::Duration;

use avian2d::{
    PhysicsPlugins,
    dynamics::integrator::Gravity,
    physics_transform::PhysicsTransformPlugin,
    prelude::{ColliderMarker, PhysicsInterpolationPlugin},
};
use bevy::prelude::*;

pub mod layers;
pub mod player;
pub mod terrain;
pub mod world;

pub use layers::GameLayer;
pub use player::{
    MoveIntent, Player, PlayerBundle, PlayerColor, PlayerSimulationBundle, PlayerSystems, Speed,
};
pub use terrain::{
    ChunkCoord, ChunkLayer, ChunkTiles, Terrain, TerrainChunk, TerrainIndex, TerrainSystems,
    TileCoord, TileEdit, TileKind, TileLayer, spawn_chunk,
};
pub use world::{SPAWN_RADIUS, spawn_point};

/// Fréquence de simulation, en ticks par seconde.
pub const TICK_HZ: u32 = 60;

/// Durée d'un tick, dérivée de [`TICK_HZ`].
pub const fn tick_duration() -> Duration {
    Duration::from_nanos(1_000_000_000 / TICK_HZ as u64)
}

/// Logique de jeu commune : physique et gameplay partagés entre client et serveur.
///
/// Ce plugin ne lit aucune entrée : il consomme uniquement le composant
/// [`MoveIntent`], que le client (clavier) ou le serveur (réseau) remplit.
///
/// **Il suppose que `ProtocolPlugin` (theta-protocole) soit ajouté ensuite** :
/// les deux plugins de synchronisation `Position`/`Rotation` ↔ `Transform`
/// d'Avian sont désactivés ici parce que `LightyearAvianPlugin` les remplace par
/// des versions compatibles avec la prédiction et l'interpolation. Sans lui,
/// `Transform` ne serait jamais mis à jour et rien ne s'afficherait.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        // Le pas fixe appartient au code commun
        app.insert_resource(Time::<Fixed>::from_duration(tick_duration()));

        app.add_plugins(
            // Le monde se mesure en pixels : l'unité de longueur ramène les
            // tolérances d'Avian (marge de `MoveAndSlide`, seuils du solveur) à
            // l'échelle d'une tile plutôt qu'à celle d'un pixel.
            PhysicsPlugins::default()
                .with_length_unit(terrain::TILE_SIZE)
                .build()
                .disable::<PhysicsTransformPlugin>()
                .disable::<PhysicsInterpolationPlugin>(),
        )
        .insert_resource(Gravity(Vec2::ZERO)) // Gravité désactivée, car le jeu est en vue du dessus
        .add_plugins((terrain::TerrainPlugin, player::PlayerPlugin));

        // Avian tire l'échelle d'un collider de son `GlobalTransform`. Sans
        // `PhysicsTransformPlugin`, rien ne l'ajoute plus : un joueur, qui n'a
        // qu'une `Position`, aurait un collider d'échelle nulle et traverserait
        // les murs jusqu'à son centre. `LightyearAvianPlugin` fait le même
        // enregistrement ; le premier arrivé l'emporte, l'autre est sans effet.
        app.try_register_required_components::<ColliderMarker, Transform>()
            .ok();
    }
}
