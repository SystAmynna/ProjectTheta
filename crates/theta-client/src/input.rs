use bevy::prelude::*;
use theta_core::MoveIntent;

use crate::LocalPlayer;

/// Traduit les entrées clavier en [`MoveIntent`] sur le joueur local.
pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KeyBindings>()
            // Avant `FixedUpdate`, pour que le mouvement du tick lise
            // une intention à jour.
            .add_systems(FixedPreUpdate, gather_move_intent);
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

fn gather_move_intent(
    mut players: Query<&mut MoveIntent, With<LocalPlayer>>,
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

    for mut intent in &mut players {
        intent.0 = dir;
    }
}
