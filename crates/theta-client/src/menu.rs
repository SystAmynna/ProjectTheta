//! Menu principal provisoire : un texte centré sur la caméra, et la touche
//! Entrée pour rejoindre le serveur de la ligne de commande.

use bevy::prelude::*;
use theta_render::GameCamera;

use crate::ClientConfig;
use crate::connection::{AppState, ConnectionError};

/// Touche qui lance la connexion depuis le menu.
const JOIN_KEY: KeyCode = KeyCode::Enter;

/// Taille du texte, en unités monde : la caméra montre toujours `VIEW_SIZE`.
const FONT_SIZE: f32 = 40.0;

pub(crate) struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), show_menu)
            .add_systems(OnEnter(AppState::Connecting), show_connecting)
            .add_systems(Update, join_on_key.run_if(in_state(AppState::Menu)));
    }
}

/// Affiche un message au centre de l'écran tant que l'étape `state` dure.
///
/// Le texte est un enfant de la caméra : il reste centré quelle que soit la
/// position à laquelle la partie précédente l'a laissée.
fn show_message(
    commands: &mut Commands,
    camera: Option<Single<Entity, With<GameCamera>>>,
    state: AppState,
    message: String,
) {
    let mut text = commands.spawn((
        Name::new("Message du menu"),
        Text2d::new(message),
        TextFont::from_font_size(FONT_SIZE),
        TextLayout::justify(Justify::Center),
        Transform::from_xyz(0.0, 0.0, 1.0),
        DespawnOnExit(state),
    ));
    if let Some(camera) = camera {
        text.insert(ChildOf(*camera));
    }
}

fn show_menu(
    mut commands: Commands,
    camera: Option<Single<Entity, With<GameCamera>>>,
    config: Res<ClientConfig>,
    error: Res<ConnectionError>,
) {
    let mut message = format!("Entrée : rejoindre {}", config.server);
    if let Some(error) = &error.0 {
        message = format!("Échec — {error}\n\n{message}");
    }
    show_message(&mut commands, camera, AppState::Menu, message);
}

fn show_connecting(
    mut commands: Commands,
    camera: Option<Single<Entity, With<GameCamera>>>,
    config: Res<ClientConfig>,
) {
    let message = format!("Connexion à {}…", config.server);
    show_message(&mut commands, camera, AppState::Connecting, message);
}

fn join_on_key(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(JOIN_KEY) {
        next.set(AppState::Connecting);
    }
}
