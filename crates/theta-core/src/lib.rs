use avian2d::{PhysicsPlugins, dynamics::integrator::Gravity};
use bevy::prelude::*;

pub mod data;
pub mod player;

pub use data::{ENEMIES, ENTITIES, EQUIPMENTS, ITEMS, Id, WEAPONS};
pub use player::{MoveIntent, Player, PlayerBundle, Speed};

/// Logique de jeu commune : physique et gameplay partagés entre client et serveur.
///
/// Ce plugin ne lit aucune entrée : il consomme uniquement le composant
/// [`MoveIntent`], que le client (clavier) ou le serveur (réseau) remplit.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        // Les tables de `data` sont figées à la compilation : il suffit de les
        // vérifier en développement, la release ne peut pas les invalider.
        #[cfg(debug_assertions)]
        data::validate();

        app.add_plugins(PhysicsPlugins::default())
            .insert_resource(Gravity(Vec2::ZERO))
            .add_plugins(player::PlayerPlugin);
    }
}
