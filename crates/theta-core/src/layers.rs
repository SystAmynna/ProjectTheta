use avian2d::prelude::PhysicsLayer;

/// Couches de collision du jeu.
#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum GameLayer {
    /// Couche par défaut d'Avian, réservée.
    #[default]
    Default,
    /// Joueurs : ils ne se bloquent pas entre eux, seulement contre le terrain.
    Player,
    /// Chunks de terrain, statiques.
    Terrain,
}
