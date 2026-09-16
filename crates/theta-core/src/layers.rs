use avian2d::prelude::PhysicsLayer;

/// Couches de collision du jeu.
///
/// Le mouvement des joueurs ne tient compte **que** de [`GameLayer::Terrain`] :
/// côté client, seul le joueur local a un collider — les joueurs interpolés n'en
/// ont pas. Si les joueurs se bloquaient entre eux sur le serveur, le client ne
/// pourrait pas le prédire et subirait des rollbacks à chaque contact.
#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum GameLayer {
    /// Couche par défaut d'Avian, réservée.
    #[default]
    Default,
    Player,
    Terrain,
}
