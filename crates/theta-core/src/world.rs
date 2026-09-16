use bevy::prelude::*;

/// Rayon du cercle sur lequel les joueurs apparaissent, en pixels.
///
/// Le monde est infini et généré par le serveur : c'est au générateur de garder
/// ce cercle praticable (voir `theta-worldgen`).
pub const SPAWN_RADIUS: f32 = 250.0;

/// Point d'apparition du n-ième joueur.
///
/// Les joueurs sont répartis sur un cercle autour de l'origine, de sorte que
/// deux joueurs ne se superposent jamais au moment de leur arrivée.
pub fn spawn_point(index: u32) -> Vec2 {
    // 8 positions avant de recommencer un tour : au-delà, l'écart angulaire
    // devient assez petit pour que les joueurs se chevauchent, mais le monde
    // n'accueille pas encore de partie assez grande pour que ça compte.
    const SLOTS: u32 = 8;

    let angle = std::f32::consts::TAU * (index % SLOTS) as f32 / SLOTS as f32;
    Vec2::from_angle(angle) * SPAWN_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_points_are_distinct_and_on_the_ring() {
        let points: Vec<Vec2> = (0..8).map(spawn_point).collect();

        for (i, a) in points.iter().enumerate() {
            assert!(
                (a.length() - SPAWN_RADIUS).abs() < 1e-3,
                "point {i} hors du cercle : {a:?}"
            );
            for b in &points[i + 1..] {
                assert!(a.distance(*b) > 1.0, "points confondus : {a:?} et {b:?}");
            }
        }
    }
}
