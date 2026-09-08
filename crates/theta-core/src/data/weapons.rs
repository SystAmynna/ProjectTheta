//! Armes : comportement de tir associé à un item de [`super::items::ITEMS`].

use super::common::DamageKind;
use super::registry::{Definition, Id, Registry};

/// Manière dont l'arme délivre ses dégâts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FireMode {
    /// Un projectile par tir.
    Projectile { speed: f32, lifetime: f32 },
    /// Plusieurs projectiles en cône, dispersion en radians.
    Spread {
        speed: f32,
        lifetime: f32,
        pellets: u8,
        angle: f32,
    },
    /// Touche instantanément, sans projectile simulé.
    Hitscan,
    /// Frappe au corps à corps, dans un arc devant le porteur.
    Melee { arc: f32 },
}

/// Définition statique d'une arme.
#[derive(Debug, Clone, Copy)]
pub struct WeaponDef {
    pub id: Id,
    /// Item correspondant dans [`super::items::ITEMS`].
    pub item: Id,
    /// Dégâts par projectile (ou par coup pour le corps à corps).
    pub damage: f32,
    pub damage_kind: DamageKind,
    /// Tirs par seconde.
    pub fire_rate: f32,
    /// Portée utile, en pixels.
    pub range: f32,
    /// Munitions par chargeur ; `None` pour une arme sans rechargement.
    pub magazine: Option<u16>,
    /// Durée de rechargement, en secondes.
    pub reload_time: f32,
    pub fire_mode: FireMode,
}

impl Definition for WeaponDef {
    const KIND: &'static str = "arme";

    fn id(&self) -> Id {
        self.id
    }
}

impl WeaponDef {
    /// Intervalle minimal entre deux tirs, en secondes.
    pub fn cooldown(&self) -> f32 {
        if self.fire_rate > 0.0 {
            1.0 / self.fire_rate
        } else {
            f32::INFINITY
        }
    }

    /// Dégâts théoriques par seconde, tir continu et rechargement ignorés.
    pub fn dps(&self) -> f32 {
        let per_shot = match self.fire_mode {
            FireMode::Spread { pellets, .. } => self.damage * pellets as f32,
            _ => self.damage,
        };
        per_shot * self.fire_rate
    }
}

/// Table des armes du jeu.
pub const WEAPONS: Registry<WeaponDef> = Registry::new(&[
    WeaponDef {
        id: Id("pistol"),
        item: Id("pistol"),
        damage: 12.0,
        damage_kind: DamageKind::Physical,
        fire_rate: 4.0,
        range: 480.0,
        magazine: Some(12),
        reload_time: 1.2,
        fire_mode: FireMode::Projectile {
            speed: 900.0,
            lifetime: 0.8,
        },
    },
    WeaponDef {
        id: Id("shotgun"),
        item: Id("shotgun"),
        damage: 7.0,
        damage_kind: DamageKind::Physical,
        fire_rate: 1.2,
        range: 260.0,
        magazine: Some(6),
        reload_time: 2.4,
        fire_mode: FireMode::Spread {
            speed: 700.0,
            lifetime: 0.4,
            pellets: 8,
            angle: core::f32::consts::FRAC_PI_8,
        },
    },
    WeaponDef {
        id: Id("arc_rifle"),
        item: Id("arc_rifle"),
        damage: 26.0,
        damage_kind: DamageKind::Lightning,
        fire_rate: 2.0,
        range: 700.0,
        magazine: Some(8),
        reload_time: 1.8,
        fire_mode: FireMode::Hitscan,
    },
]);
