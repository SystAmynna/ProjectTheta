//! Types partagés par plusieurs tables de données.

/// Rareté d'un item, d'une arme ou d'un équipement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// Couleur d'affichage associée, en sRGB linéaire par composante.
    ///
    /// `theta-core` ne connaît pas le rendu : c'est à `theta-render` d'en faire
    /// une `Color` Bevy (`Color::srgb(r, g, b)`).
    pub const fn rgb(self) -> [f32; 3] {
        match self {
            Rarity::Common => [0.78, 0.78, 0.78],
            Rarity::Uncommon => [0.35, 0.78, 0.35],
            Rarity::Rare => [0.30, 0.55, 0.95],
            Rarity::Epic => [0.65, 0.35, 0.90],
            Rarity::Legendary => [0.95, 0.65, 0.20],
        }
    }
}

/// Nature des dégâts infligés, pour les résistances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DamageKind {
    #[default]
    Physical,
    Fire,
    Ice,
    Lightning,
    Poison,
}

/// Camp d'une entité : détermine qui peut blesser qui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Faction {
    #[default]
    Neutral,
    Player,
    Hostile,
}

/// Emplacement d'équipement sur un personnage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EquipmentSlot {
    Head,
    Chest,
    Legs,
    Feet,
    Hands,
    Trinket,
}

impl EquipmentSlot {
    /// Tous les emplacements, dans l'ordre d'affichage.
    pub const ALL: &'static [EquipmentSlot] = &[
        EquipmentSlot::Head,
        EquipmentSlot::Chest,
        EquipmentSlot::Legs,
        EquipmentSlot::Feet,
        EquipmentSlot::Hands,
        EquipmentSlot::Trinket,
    ];
}

/// Modificateurs de caractéristiques, additionnés puis appliqués à l'entité.
///
/// Les champs sont des deltas : `Stats::NONE` est l'élément neutre, ce qui
/// permet de sommer l'équipement porté sans cas particulier.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    /// Points de vie maximum ajoutés.
    pub max_health: f32,
    /// Vitesse de déplacement ajoutée, en pixels par seconde.
    pub speed: f32,
    /// Dégâts ajoutés à chaque coup porté.
    pub damage: f32,
    /// Dégâts absorbés à chaque coup reçu.
    pub armor: f32,
}

impl Stats {
    pub const NONE: Stats = Stats {
        max_health: 0.0,
        speed: 0.0,
        damage: 0.0,
        armor: 0.0,
    };

    /// Somme de deux jeux de modificateurs.
    pub const fn plus(self, other: Stats) -> Stats {
        Stats {
            max_health: self.max_health + other.max_health,
            speed: self.speed + other.speed,
            damage: self.damage + other.damage,
            armor: self.armor + other.armor,
        }
    }
}

impl core::ops::Add for Stats {
    type Output = Stats;

    fn add(self, other: Stats) -> Stats {
        self.plus(other)
    }
}

impl core::iter::Sum for Stats {
    fn sum<I: Iterator<Item = Stats>>(iter: I) -> Stats {
        iter.fold(Stats::NONE, Stats::plus)
    }
}
