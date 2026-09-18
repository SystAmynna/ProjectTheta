use bevy::prelude::*;
use lightyear::prelude::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use theta_protocole::MoveInput;

/// Traduit les entrées clavier en [`MoveInput`] sur le joueur local.
pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        // La saisie a lieu à chaque tick, juste avant la simulation, dans le set
        // où lightyear attend les entrées du client : il les y bufferise, les
        // envoie au serveur et les rejoue lors des rollbacks.
        app.init_resource::<KeyBindings>().add_systems(
            FixedPreUpdate,
            gather_move_input.in_set(InputSystems::WriteClientInputs),
        );
    }
}

/// Touches de déplacement, modifiables à l'exécution.
#[derive(Resource, Debug, Clone)]
pub struct KeyBindings {
    pub up: KeyCode,
    pub down: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
}

/// Disposition ZQSD/WASD selon la position physique des touches.
impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            up: KeyCode::KeyW,
            down: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
        }
    }
}

/// Lit le clavier et écrit la direction normalisée dans l'`ActionState` du
/// joueur local.
fn gather_move_input(
    mut players: Query<&mut ActionState<MoveInput>, With<InputMarker<MoveInput>>>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<KeyBindings>,
) {
    let mut dir = Vec2::ZERO;

    if keys.pressed(bindings.up) {
        dir.y += 1.0;
    }
    if keys.pressed(bindings.down) {
        dir.y -= 1.0;
    }
    if keys.pressed(bindings.left) {
        dir.x -= 1.0;
    }
    if keys.pressed(bindings.right) {
        dir.x += 1.0;
    }

    let dir = dir.normalize_or_zero();

    for mut action in &mut players {
        action.0 = MoveInput(dir);
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    /// Fait tourner `gather_move_input` avec les touches `pressed` enfoncées et
    /// renvoie la direction écrite sur le joueur local.
    fn gather(pressed: &[KeyCode]) -> Vec2 {
        let mut world = World::new();
        world.init_resource::<KeyBindings>();

        let mut keys = ButtonInput::<KeyCode>::default();
        for &key in pressed {
            keys.press(key);
        }
        world.insert_resource(keys);

        let player = world
            .spawn((
                InputMarker::<MoveInput>::default(),
                ActionState::<MoveInput>::default(),
            ))
            .id();
        world.run_system_once(gather_move_input).unwrap();

        world.get::<ActionState<MoveInput>>(player).unwrap().0.0
    }

    #[test]
    fn single_key_gives_a_unit_direction() {
        assert_eq!(gather(&[KeyCode::KeyW]), Vec2::Y);
        assert_eq!(gather(&[KeyCode::KeyA]), Vec2::NEG_X);
    }

    #[test]
    fn diagonal_is_normalized() {
        let direction = gather(&[KeyCode::KeyW, KeyCode::KeyD]);
        assert!((direction.length() - 1.0).abs() < 1e-6, "{direction:?}");
        assert!(direction.x > 0.0 && direction.y > 0.0, "{direction:?}");
    }

    #[test]
    fn opposite_keys_cancel_out() {
        assert_eq!(gather(&[KeyCode::KeyW, KeyCode::KeyS]), Vec2::ZERO);
        assert_eq!(gather(&[]), Vec2::ZERO);
    }
}
