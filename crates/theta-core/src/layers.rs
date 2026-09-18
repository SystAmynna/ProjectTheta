use avian2d::prelude::PhysicsLayer;

/// Couches de collision du jeu
#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub enum GameLayer {
    /// Couche par défaut d'Avian, réservée.
    #[default]
    Default,
    Player,
    Terrain,
    // todo : ajouter un layer pour les projectiles (et les hitbox)
}
