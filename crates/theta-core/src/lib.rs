use avian2d::{
    PhysicsPlugins, dynamics::integrator::Gravity, physics_transform::PhysicsTransformPlugin,
    prelude::PhysicsInterpolationPlugin,
};
use bevy::prelude::*;

//pub mod data;
pub mod player;
pub mod world;

//pub use data::{ENEMIES, ENTITIES, EQUIPMENTS, ITEMS, Id, WEAPONS};
pub use player::{
    MoveIntent, Player, PlayerBundle, PlayerColor, PlayerSimulationBundle, PlayerSystems, Speed,
};
pub use world::GameWorld;

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
        // Les tables de `data` sont figées à la compilation : il suffit de les
        // vérifier en développement, la release ne peut pas les invalider.
        //#[cfg(debug_assertions)]
        //data::validate();

        app.add_plugins(
            PhysicsPlugins::default()
                .build()
                .disable::<PhysicsTransformPlugin>()
                .disable::<PhysicsInterpolationPlugin>(),
        )
        .insert_resource(Gravity(Vec2::ZERO))
        .add_plugins((world::WorldPlugin, player::PlayerPlugin));
    }
}
