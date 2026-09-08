use bevy::prelude::*;

/// Protocole réseau partagé : composants répliqués, inputs et messages
/// communs au client et au serveur (lightyear).
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, _app: &mut App) {
        // TODO: enregistrer les composants répliqués, les inputs et les messages.
    }
}
