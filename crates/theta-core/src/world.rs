use avian2d::prelude::{LinearVelocity, Position};
use bevy::prelude::*;

use crate::player::{Player, PlayerSystems};

/// Le monde de jeu : pour l'instant, une simple aire rectangulaire vide.
///
/// Il ne contient aucun décor — seuls les joueurs y vivent. C'est ici que
/// viendront la carte et les obstacles ; en attendant, il fixe les bornes du
/// terrain et répartit les points d'apparition.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameWorld>()
            // Juste après le mouvement, et donc avant qu'Avian n'intègre la
            // vélocité en position.
            .add_systems(FixedUpdate, confine_players.after(PlayerSystems::Move));
    }
}

/// Dimensions du monde jouable, en pixels, centré sur l'origine.
#[derive(Resource, Debug, Clone, Copy)]
pub struct GameWorld {
    /// Demi-largeur et demi-hauteur : le monde va de `-half_extents` à `+half_extents`.
    pub half_extents: Vec2,
}

impl Default for GameWorld {
    fn default() -> Self {
        Self {
            half_extents: Vec2::splat(Self::DEFAULT_HALF_EXTENT),
        }
    }
}

impl GameWorld {
    /// Demi-côté par défaut : un terrain de 2000 × 2000 pixels.
    pub const DEFAULT_HALF_EXTENT: f32 = 1000.0;

    /// Rayon du cercle sur lequel les joueurs apparaissent.
    const SPAWN_RING_RATIO: f32 = 0.25;

    /// Point d'apparition du n-ième joueur.
    ///
    /// Les joueurs sont répartis sur un cercle autour de l'origine, de sorte que
    /// deux joueurs ne se superposent jamais au moment de leur arrivée.
    pub fn spawn_point(&self, index: u32) -> Vec2 {
        // 8 positions avant de recommencer un tour : au-delà, l'écart angulaire
        // devient assez petit pour que les joueurs se chevauchent, mais le monde
        // n'accueille pas encore de partie assez grande pour que ça compte.
        const SLOTS: u32 = 8;

        let angle = std::f32::consts::TAU * (index % SLOTS) as f32 / SLOTS as f32;
        let radius = self.half_extents.min_element() * Self::SPAWN_RING_RATIO;

        Vec2::from_angle(angle) * radius
    }

    /// Ramène une position à l'intérieur des bornes du monde.
    pub fn clamp(&self, position: Vec2) -> Vec2 {
        position.clamp(-self.half_extents, self.half_extents)
    }
}

/// Maintient les joueurs à l'intérieur du monde.
///
/// Replacer la position ne suffit pas : la vélocité qui a poussé le joueur
/// dehors est intégrée par Avian juste après, et le ferait ressortir au tick
/// suivant. Le résultat serait une oscillation d'un tick le long du mur — et,
/// côté client, une divergence permanente avec le serveur qui provoquerait des
/// rollbacks en continu. On annule donc aussi la composante de vélocité qui
/// pointe vers l'extérieur.
fn confine_players(
    world: Res<GameWorld>,
    mut players: Query<(&mut Position, &mut LinearVelocity), With<Player>>,
) {
    for (mut position, mut velocity) in &mut players {
        confine_axis(
            &mut position.0.x,
            &mut velocity.0.x,
            world.half_extents.x,
        );
        confine_axis(
            &mut position.0.y,
            &mut velocity.0.y,
            world.half_extents.y,
        );
    }
}

/// Borne une coordonnée et neutralise la vitesse qui la pousse vers le mur.
fn confine_axis(position: &mut f32, velocity: &mut f32, limit: f32) {
    if *position >= limit {
        *position = limit;
        *velocity = velocity.min(0.0);
    } else if *position <= -limit {
        *position = -limit;
        *velocity = velocity.max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_points_are_distinct_and_inside_the_world() {
        let world = GameWorld::default();
        let points: Vec<Vec2> = (0..8).map(|i| world.spawn_point(i)).collect();

        for (i, a) in points.iter().enumerate() {
            assert_eq!(world.clamp(*a), *a, "point {i} hors du monde : {a:?}");
            for b in &points[i + 1..] {
                assert!(a.distance(*b) > 1.0, "points confondus : {a:?} et {b:?}");
            }
        }
    }

    #[test]
    fn confine_stops_a_player_against_the_wall() {
        let limit = 100.0;
        let (mut position, mut velocity) = (150.0, 300.0);

        confine_axis(&mut position, &mut velocity, limit);
        assert_eq!(position, limit);
        assert_eq!(velocity, 0.0, "la vitesse sortante doit être annulée");

        // Une fois au mur, repartir vers l'intérieur reste possible.
        let mut inward = -300.0;
        confine_axis(&mut position, &mut inward, limit);
        assert_eq!(inward, -300.0);
    }

    #[test]
    fn clamp_keeps_positions_inside() {
        let world = GameWorld::default();
        let outside = Vec2::new(10_000.0, -10_000.0);
        assert_eq!(world.clamp(outside), Vec2::new(1000.0, -1000.0));
    }
}
