//! Équipements : pièces portées sur un emplacement, qui modifient les stats.

use super::common::{DamageKind, EquipmentSlot, Stats};
use super::registry::{Definition, Id, Registry};

/// Réduction de dégâts pour une nature de dégâts donnée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resistance {
    pub kind: DamageKind,
    /// Fraction de dégâts absorbée, dans `0.0..=1.0`.
    pub ratio: f32,
}

/// Définition statique d'un équipement.
#[derive(Debug, Clone, Copy)]
pub struct EquipmentDef {
    pub id: Id,
    /// Item correspondant dans [`super::items::ITEMS`].
    pub item: Id,
    pub slot: EquipmentSlot,
    /// Modificateurs appliqués tant que la pièce est portée.
    pub stats: Stats,
    pub resistances: &'static [Resistance],
}

impl Definition for EquipmentDef {
    const KIND: &'static str = "équipement";

    fn id(&self) -> Id {
        self.id
    }
}

impl EquipmentDef {
    /// Fraction absorbée pour une nature de dégâts, `0.0` si non couverte.
    pub fn resistance(&self, kind: DamageKind) -> f32 {
        self.resistances
            .iter()
            .find(|r| r.kind == kind)
            .map_or(0.0, |r| r.ratio)
    }
}

/// Table des équipements du jeu.
pub const EQUIPMENTS: Registry<EquipmentDef> = Registry::new(&[
    EquipmentDef {
        id: Id("scrap_helmet"),
        item: Id("scrap_helmet"),
        slot: EquipmentSlot::Head,
        stats: Stats {
            max_health: 10.0,
            armor: 2.0,
            ..Stats::NONE
        },
        resistances: &[],
    },
    EquipmentDef {
        id: Id("plated_vest"),
        item: Id("plated_vest"),
        slot: EquipmentSlot::Chest,
        stats: Stats {
            max_health: 25.0,
            armor: 6.0,
            speed: -20.0,
            ..Stats::NONE
        },
        resistances: &[Resistance {
            kind: DamageKind::Physical,
            ratio: 0.15,
        }],
    },
    EquipmentDef {
        id: Id("runner_boots"),
        item: Id("runner_boots"),
        slot: EquipmentSlot::Feet,
        stats: Stats {
            speed: 60.0,
            ..Stats::NONE
        },
        resistances: &[],
    },
]);
