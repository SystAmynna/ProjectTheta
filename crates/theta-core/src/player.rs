use avian2d::prelude::{
    Collider, CollisionLayers, CustomPositionIntegration, LinearVelocity, MoveAndSlide,
    MoveAndSlideConfig, MoveAndSlideHitResponse, Position, RigidBody, Rotation,
    SpatialQueryFilter,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::layers::GameLayer;

/// Systèmes de gameplay du joueur, partagés entre client et serveur.
pub(crate) struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(FixedUpdate, PlayerSystems::Move.before(PlayerSystems::Collide))
            .add_systems(
                FixedUpdate,
                (
                    apply_move_intent.in_set(PlayerSystems::Move),
                    slide_players.in_set(PlayerSystems::Collide),
                ),
            );
    }
}

/// Étapes du gameplay du joueur, pour que les autres crates puissent s'ordonner
/// autour sans connaître les systèmes eux-mêmes.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerSystems {
    /// Traduction de [`MoveIntent`] en vélocité désirée.
    Move,
    /// Déplacement effectif, qui glisse le long du terrain.
    Collide,
}

/// Marque une entité contrôlable comme joueur.
///
/// Ce marqueur est répliqué : il porte, à lui seul, le fait qu'une entité reçue
/// du réseau est un joueur — y compris celles que le client ne fait
/// qu'interpoler et qui n'ont donc aucun composant de simulation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Player;

/// Couleur d'un joueur, en RGB linéaire.
///
/// `theta-core` ne dépend pas du rendu : comme [`Rarity::rgb`](crate::data::Rarity::rgb),
/// il expose des composantes brutes et laisse `theta-render` en faire une couleur
/// Bevy. La couleur est décidée par le serveur et répliquée, pour que tous les
/// clients voient le même joueur de la même teinte.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerColor(pub [f32; 3]);

impl Default for PlayerColor {
    fn default() -> Self {
        Self([1.0, 1.0, 1.0])
    }
}

/// Vitesse de déplacement, en pixels par seconde.
#[derive(Component, Debug, Clone, Copy)]
pub struct Speed(pub f32);

impl Default for Speed {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl Speed {
    pub const DEFAULT: f32 = 300.0;
}

/// Direction de déplacement souhaitée pour le tick courant.
///
/// C'est le seul point d'entrée du mouvement : le client la remplit à partir
/// du clavier, le serveur à partir des entrées répliquées.
#[derive(Component, Debug, Clone, Copy, Default, Deref, DerefMut)]
pub struct MoveIntent(pub Vec2);

impl MoveIntent {
    /// Direction normalisée (vecteur nul si aucune intention).
    pub fn direction(&self) -> Vec2 {
        self.0.normalize_or_zero()
    }
}

/// Composants de **simulation** d'un joueur : ceux qui ne viennent pas du
/// réseau et qu'il faut donc poser à la main de chaque côté qui simule.
///
/// Le joueur est un corps **cinématique** : rien ne le pousse, et Avian ne
/// l'arrête pas non plus contre un mur — un cinématique traverse les statiques.
/// C'est donc [`slide_players`] qui le déplace, en glissant le long du terrain ;
/// `CustomPositionIntegration` empêche Avian d'intégrer la vélocité une seconde
/// fois par-dessus.
///
/// Ce bundle n'a de sens que sur les entités simulées : celle du serveur et sa
/// copie prédite côté client. Il ne doit **jamais** être posé sur une entité
/// interpolée — la physique n'a rien à y faire, et le `RigidBody` l'afficherait
/// à l'origine tant que la première interpolation n'est pas arrivée.
#[derive(Bundle)]
pub struct PlayerSimulationBundle {
    pub player: Player,
    pub speed: Speed,
    pub intent: MoveIntent,
    pub body: RigidBody,
    pub collider: Collider,
    pub layers: CollisionLayers,
    pub integration: CustomPositionIntegration,
}

impl PlayerSimulationBundle {
    /// Rayon de la hitbox du joueur, en pixels (le sprite fait 64 × 64).
    pub const RADIUS: f32 = 32.0;

    pub fn new(speed: f32) -> Self {
        Self {
            player: Player,
            speed: Speed(speed),
            intent: MoveIntent::default(),
            body: RigidBody::Kinematic,
            collider: Collider::circle(Self::RADIUS),
            layers: CollisionLayers::new(GameLayer::Player, GameLayer::Terrain),
            integration: CustomPositionIntegration,
        }
    }
}

/// Un joueur complet : simulation **et** pose initiale.
///
/// C'est ce que spawne le serveur, qui fait autorité sur la position. Le client
/// n'utilise que [`PlayerSimulationBundle`] : sa pose lui arrive par
/// réplication, et l'écraser localement annulerait la correction du serveur.
#[derive(Bundle)]
pub struct PlayerBundle {
    pub simulation: PlayerSimulationBundle,
    pub position: Position,
    pub rotation: Rotation,
    pub velocity: LinearVelocity,
}

impl PlayerBundle {
    pub fn new(position: Vec2, speed: f32) -> Self {
        Self {
            simulation: PlayerSimulationBundle::new(speed),
            position: Position(position),
            rotation: Rotation::default(),
            velocity: LinearVelocity::default(),
        }
    }
}

/// Traduit l'intention de déplacement de chaque joueur en vélocité désirée.
///
/// Ce système ne touche ni à `Position` ni à `Transform` : c'est
/// [`slide_players`] qui en fait un déplacement.
fn apply_move_intent(
    mut players: Query<(&mut LinearVelocity, &Speed, &MoveIntent), With<Player>>,
) {
    for (mut velocity, speed, intent) in &mut players {
        velocity.0 = intent.direction() * speed.0;
    }
}

/// Un joueur simulé ici : celui du serveur, ou le joueur prédit du client. Les
/// joueurs interpolés n'ont pas de `MoveIntent` et ne doivent pas être déplacés.
type SimulatedPlayer = (With<Player>, With<MoveIntent>);

/// Ce que [`slide_players`] écrit sur les joueurs simulés.
type SimulatedPlayers<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Position, &'static mut LinearVelocity),
    SimulatedPlayer,
>;

/// Déplace chaque joueur simulé selon sa vélocité, en glissant le long du terrain.
///
/// `MoveAndSlide` avance le collider jusqu'au premier obstacle, puis projette la
/// vitesse restante sur le mur pour longer celui-ci. La vélocité écrite en retour
/// est cette vitesse projetée : elle ne pointe jamais dans un mur, sans quoi le
/// joueur y serait repoussé à chaque tick et, côté client, divergerait du serveur
/// en déclenchant des rollbacks en continu.
///
/// Seul [`GameLayer::Terrain`] est pris en compte (voir [`GameLayer`]). Le calcul
/// est déterministe : rejoué lors d'un rollback, il redonne la même position.
///
/// `MoveAndSlide` lit `Position` sur tous les colliders, joueurs compris : on ne
/// peut pas tenir en même temps un accès mutable aux joueurs, d'où le
/// `ParamSet` et les trois temps — relever, calculer, écrire.
fn slide_players(
    mut params: ParamSet<(SimulatedPlayers, MoveAndSlide)>,
    time: Res<Time>,
    mut moves: Local<Vec<(Entity, Vec2, Vec2)>>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Terrain);
    let config = MoveAndSlideConfig::default();

    moves.clear();
    moves.extend(
        params
            .p0()
            .iter()
            .map(|(entity, _, velocity)| (entity, Vec2::ZERO, velocity.0)),
    );

    let move_and_slide = params.p1();
    moves.retain_mut(|(entity, destination, velocity)| {
        // Un joueur dont le collider n'est pas encore attaché reste où il est.
        let Ok((collider, position, rotation, _)) = move_and_slide.colliders.get(*entity) else {
            return false;
        };

        let out = move_and_slide.move_and_slide(
            collider,
            position.0,
            rotation.as_radians(),
            *velocity,
            time.delta(),
            &config,
            &filter,
            |_| MoveAndSlideHitResponse::Accept,
        );
        *destination = out.position;
        *velocity = out.projected_velocity;
        true
    });

    let mut players = params.p0();
    for &(entity, destination, velocity) in moves.iter() {
        if let Ok((_, mut position, mut linear)) = players.get_mut(entity) {
            position.0 = destination;
            linear.0 = velocity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fait tourner `apply_move_intent` sur un joueur et renvoie sa vélocité.
    fn run(intent: Vec2, speed: f32) -> Vec2 {
        let mut world = World::new();

        let entity = world
            .spawn((
                Player,
                Speed(speed),
                MoveIntent(intent),
                LinearVelocity::default(),
            ))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_move_intent);
        schedule.run(&mut world);

        world.get::<LinearVelocity>(entity).unwrap().0
    }

    #[test]
    fn move_intent_sets_velocity_to_speed() {
        assert_eq!(run(Vec2::X, 100.0), Vec2::new(100.0, 0.0));
    }

    #[test]
    fn diagonal_intent_is_normalized() {
        let velocity = run(Vec2::ONE, 100.0);
        assert!(
            (velocity.length() - 100.0).abs() < 1e-3,
            "longueur {velocity:?}"
        );
    }

    #[test]
    fn no_intent_stops_the_player() {
        assert_eq!(run(Vec2::ZERO, 100.0), Vec2::ZERO);
    }
}
