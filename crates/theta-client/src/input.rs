use bevy::prelude::*;
use lightyear::prelude::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use theta_protocole::MoveInput;

/// Traduit les entrées clavier en [`MoveInput`] sur le joueur local.
pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KeyBindings>()
            // `WriteClientInputs` est le moment prévu par lightyear pour écrire
            // l'`ActionState` : c'est juste avant qu'il ne soit bufferisé, puis
            // envoyé au serveur et rejoué en cas de rollback.
            .add_systems(
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
