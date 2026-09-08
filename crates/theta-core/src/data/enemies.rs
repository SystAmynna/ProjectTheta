//! Ennemis : comportement, récompenses et butin greffés sur une entité.

use super::registry::{Definition, Id, Registry};

/// Comportement de déplacement et d'attaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Behavior {
    /// Fonce sur la cible la plus proche et frappe au contact.
    Charger,
    /// Garde ses distances et tire à `range` pixels.
    Ranged { range: f32 },
    /// Immobile jusqu'à ce qu'une cible entre dans `trigger_range`.
    Ambusher { trigger_range: f32 },
}

/// Une ligne de table de butin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LootEntry {
    /// Item lâché, dans [`super::items::ITEMS`].
    pub item: Id,
    /// Probabilité de la ligne, dans `0.0..=1.0`.
    pub chance: f32,
    /// Quantité tirée uniformément dans `min..=max`.
    pub min: u16,
    pub max: u16,
}

/// Définition statique d'un ennemi.
#[derive(Debug, Clone, Copy)]
pub struct EnemyDef {
    pub id: Id,
    /// Corps de l'ennemi, dans [`super::entities::ENTITIES`].
    pub entity: Id,
    pub behavior: Behavior,
    /// Dégâts de son attaque.
    pub damage: f32,
    /// Attaques par seconde.
    pub attack_rate: f32,
    /// Arme utilisée, pour les ennemis à distance.
    pub weapon: Option<Id>,
    /// Expérience accordée à la mort.
    pub experience: u32,
    pub loot: &'static [LootEntry],
}

impl Definition for EnemyDef {
    const KIND: &'static str = "ennemi";

    fn id(&self) -> Id {
        self.id
    }
}

/// Table des ennemis du jeu.
pub const ENEMIES: Registry<EnemyDef> = Registry::new(&[
    EnemyDef {
        id: Id("crawler"),
        entity: Id("crawler"),
        behavior: Behavior::Charger,
        damage: 8.0,
        attack_rate: 1.5,
        weapon: None,
        experience: 5,
        loot: &[LootEntry {
            item: Id("scrap"),
            chance: 0.6,
            min: 1,
            max: 3,
        }],
    },
    EnemyDef {
        id: Id("spitter"),
        entity: Id("spitter"),
        behavior: Behavior::Ranged { range: 320.0 },
        damage: 12.0,
        attack_rate: 0.8,
        weapon: Some(Id("pistol")),
        experience: 9,
        loot: &[
            LootEntry {
                item: Id("scrap"),
                chance: 0.5,
                min: 1,
                max: 2,
            },
            LootEntry {
                item: Id("energy_cell"),
                chance: 0.25,
                min: 1,
                max: 1,
            },
        ],
    },
    EnemyDef {
        id: Id("brute"),
        entity: Id("brute"),
        behavior: Behavior::Ambusher {
            trigger_range: 220.0,
        },
        damage: 30.0,
        attack_rate: 0.5,
        weapon: None,
        experience: 40,
        loot: &[
            LootEntry {
                item: Id("scrap"),
                chance: 1.0,
                min: 3,
                max: 6,
            },
            LootEntry {
                item: Id("medkit"),
                chance: 0.35,
                min: 1,
                max: 1,
            },
            LootEntry {
                item: Id("plated_vest"),
                chance: 0.08,
                min: 1,
                max: 1,
            },
        ],
    },
]);
