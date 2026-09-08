use std::time::Duration;

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

/// Fréquence de simulation, en ticks par seconde.
///
/// C'est **la** constante d'accord entre les deux camps : elle définit l'unité
/// de temps du réseau (un tick = un input) et le pas de la physique. Elle est
/// figée ici, dans le code commun, précisément pour que ni le client ni le
/// serveur ne puissent en choisir une autre — pas d'option, pas de ressource,
/// pas d'argument de ligne de commande. La changer, c'est changer le protocole.
///
/// Elle ne dit rien du nombre d'images par seconde : le client rend en `Update`
/// aussi vite que sa machine le permet, et n'exécute `FixedUpdate` qu'à cette
/// cadence-ci.
pub const TICK_HZ: u32 = 60;

/// Durée d'un tick, dérivée de [`TICK_HZ`].
///
/// Client et serveur passent tous deux par cette fonction : l'arrondi à la
/// nanoseconde est donc rigoureusement le même des deux côtés.
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
        // Les tables de `data` sont figées à la compilation : il suffit de les
        // vérifier en développement, la release ne peut pas les invalider.
        //#[cfg(debug_assertions)]
        //data::validate();

        // Le pas fixe appartient au code commun. Lightyear le règle aussi
        // depuis le `tick_duration` de ses propres plugins, mais les deux
        // binaires l'alimentent avec [`tick_duration`] : les valeurs coïncident,
        // et cette insertion garantit la cadence même sans réseau.
        app.insert_resource(Time::<Fixed>::from_duration(tick_duration()));

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
