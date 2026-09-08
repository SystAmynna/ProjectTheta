use bevy::prelude::*;

/// Systèmes de gameplay du joueur, partagés entre client et serveur.
pub(crate) struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, apply_move_intent);
    }
}

/// Marque une entité contrôlable comme joueur.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Player;

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

/// Ensemble minimal de composants d'un joueur.
#[derive(Bundle, Default)]
pub struct PlayerBundle {
    pub player: Player,
    pub speed: Speed,
    pub intent: MoveIntent,
    pub transform: Transform,
}

impl PlayerBundle {
    pub fn new(position: Vec2, speed: f32) -> Self {
        Self {
            speed: Speed(speed),
            transform: Transform::from_translation(position.extend(0.0)),
            ..default()
        }
    }
}

/// Applique l'intention de déplacement de chaque joueur à sa position.
fn apply_move_intent(
    mut query: Query<(&mut Transform, &Speed, &MoveIntent), With<Player>>,
    time: Res<Time>,
) {
    let delta = time.delta_secs();

    for (mut transform, speed, intent) in &mut query {
        let displacement = intent.direction() * speed.0 * delta;
        transform.translation += displacement.extend(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fait tourner `apply_move_intent` sur un joueur, avec un delta fixe.
    fn run(intent: Vec2, speed: f32, delta: f32) -> Vec2 {
        let mut world = World::new();

        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs_f32(delta));
        world.insert_resource(time);

        let entity = world
            .spawn((
                Player,
                Speed(speed),
                MoveIntent(intent),
                Transform::default(),
            ))
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_move_intent);
        schedule.run(&mut world);

        world
            .get::<Transform>(entity)
            .unwrap()
            .translation
            .truncate()
    }

    #[test]
    fn move_intent_moves_at_speed() {
        let moved = run(Vec2::X, 100.0, 0.5);
        assert_eq!(moved, Vec2::new(50.0, 0.0));
    }

    #[test]
    fn diagonal_intent_is_normalized() {
        let moved = run(Vec2::ONE, 100.0, 1.0);
        assert!((moved.length() - 100.0).abs() < 1e-3, "longueur {moved:?}");
    }

    #[test]
    fn no_intent_does_not_move() {
        assert_eq!(run(Vec2::ZERO, 100.0, 1.0), Vec2::ZERO);
    }
}
