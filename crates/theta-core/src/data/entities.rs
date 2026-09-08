//! Entités : caractéristiques de base de tout ce qui vit et occupe l'espace.
//!
//! Un ennemi ([`super::enemies`]) référence une entité pour son corps, et n'y
//! ajoute que son comportement et ses récompenses.

use super::common::Faction;
use super::registry::{Definition, Id, Registry};

/// Forme de collision, en pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hitbox {
    Circle { radius: f32 },
    Rect { half_width: f32, half_height: f32 },
}

/// Définition statique d'une entité.
#[derive(Debug, Clone, Copy)]
pub struct EntityDef {
    pub id: Id,
    pub name: &'static str,
    pub faction: Faction,
    pub max_health: f32,
    /// Vitesse de déplacement de base, en pixels par seconde.
    pub speed: f32,
    pub hitbox: Hitbox,
    /// Chemin du sprite, relatif à `assets/`.
    pub sprite: &'static str,
}

impl Definition for EntityDef {
    const KIND: &'static str = "entité";

    fn id(&self) -> Id {
        self.id
    }
}

/// Entité jouable de référence : le joueur.
pub const PLAYER: Id = Id("player");

/// Table des entités du jeu.
pub const ENTITIES: Registry<EntityDef> = Registry::new(&[
    EntityDef {
        id: PLAYER,
        name: "Joueur",
        faction: Faction::Player,
        max_health: 100.0,
        speed: crate::player::Speed::DEFAULT,
        hitbox: Hitbox::Circle { radius: 12.0 },
        sprite: "a.png",
    },
    EntityDef {
        id: Id("crawler"),
        name: "Rampant",
        faction: Faction::Hostile,
        max_health: 30.0,
        speed: 160.0,
        hitbox: Hitbox::Circle { radius: 10.0 },
        sprite: "entities/crawler.png",
    },
    EntityDef {
        id: Id("spitter"),
        name: "Cracheur",
        faction: Faction::Hostile,
        max_health: 45.0,
        speed: 110.0,
        hitbox: Hitbox::Circle { radius: 12.0 },
        sprite: "entities/spitter.png",
    },
    EntityDef {
        id: Id("brute"),
        name: "Brute",
        faction: Faction::Hostile,
        max_health: 260.0,
        speed: 90.0,
        hitbox: Hitbox::Rect {
            half_width: 22.0,
            half_height: 26.0,
        },
        sprite: "entities/brute.png",
    },
    EntityDef {
        id: Id("crate"),
        name: "Caisse",
        faction: Faction::Neutral,
        max_health: 20.0,
        speed: 0.0,
        hitbox: Hitbox::Rect {
            half_width: 16.0,
            half_height: 16.0,
        },
        sprite: "entities/crate.png",
    },
]);
