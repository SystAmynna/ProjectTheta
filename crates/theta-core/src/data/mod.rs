//! Données statiques du jeu : items, armes, équipements, entités, ennemis.
//!
//! Tout est déclaré en `const` dans ces fichiers et intégré au binaire à la
//! compilation : aucun fichier à charger, aucun parsing au démarrage, et les
//! fautes de frappe dans les *champs* sont des erreurs de compilation.
//!
//! Les tables se référencent entre elles par [`Id`] plutôt que par pointeur,
//! pour rester déclarables en `const` et sérialisables telles quelles. Ces
//! références sont vérifiées au démarrage par [`validate`], appelée par
//! [`CorePlugin`](crate::CorePlugin) en `debug_assertions`.
//!
//! ```
//! use theta_core::data::{items::ITEMS, registry::Id, weapons::WEAPONS};
//!
//! let pistol = WEAPONS.expect(Id("pistol"));
//! let item = ITEMS.expect(pistol.item);
//! assert_eq!(item.name, "Pistolet");
//! ```
//!
//! ## Ajouter une donnée
//!
//! 1. ajouter l'entrée dans la table du fichier concerné ;
//! 2. si elle en référence une autre (un item pour une arme, une entité pour un
//!    ennemi), créer aussi l'entrée cible ;
//! 3. `cargo test -p theta-core` : les tests vérifient l'unicité des
//!    identifiants et la résolution de toutes les références.

pub mod common;
pub mod enemies;
pub mod entities;
pub mod equipments;
pub mod items;
pub mod registry;
pub mod weapons;

pub use common::{DamageKind, EquipmentSlot, Faction, Rarity, Stats};
pub use enemies::{ENEMIES, EnemyDef};
pub use entities::{ENTITIES, EntityDef};
pub use equipments::{EQUIPMENTS, EquipmentDef};
pub use items::{ITEMS, ItemDef, ItemKind};
pub use registry::{Definition, Id, Registry};
pub use weapons::{WEAPONS, WeaponDef};

/// Vérifie la cohérence de toutes les tables : identifiants uniques et
/// références croisées résolubles.
///
/// Panique au premier problème. Appelée automatiquement par
/// [`CorePlugin`](crate::CorePlugin) en build de développement, et par les
/// tests ; inutile en release, où les tables n'ont pas pu changer.
pub fn validate() {
    ITEMS.assert_unique_ids();
    WEAPONS.assert_unique_ids();
    EQUIPMENTS.assert_unique_ids();
    ENTITIES.assert_unique_ids();
    ENEMIES.assert_unique_ids();

    for item in &ITEMS {
        assert!(
            item.max_stack >= 1,
            "item `{}` : max_stack doit valoir au moins 1",
            item.id
        );

        match item.kind {
            ItemKind::Weapon(weapon) => assert!(
                WEAPONS.contains(weapon),
                "item `{}` référence l'arme inconnue `{weapon}`",
                item.id
            ),
            ItemKind::Equipment(equipment) => assert!(
                EQUIPMENTS.contains(equipment),
                "item `{}` référence l'équipement inconnu `{equipment}`",
                item.id
            ),
            ItemKind::Material | ItemKind::Consumable(_) => {}
        }
    }

    for weapon in &WEAPONS {
        assert!(
            ITEMS.contains(weapon.item),
            "arme `{}` référence l'item inconnu `{}`",
            weapon.id,
            weapon.item
        );
        assert!(
            weapon.fire_rate > 0.0,
            "arme `{}` : fire_rate doit être strictement positif",
            weapon.id
        );
    }

    for equipment in &EQUIPMENTS {
        assert!(
            ITEMS.contains(equipment.item),
            "équipement `{}` référence l'item inconnu `{}`",
            equipment.id,
            equipment.item
        );

        for resistance in equipment.resistances {
            assert!(
                (0.0..=1.0).contains(&resistance.ratio),
                "équipement `{}` : résistance hors de 0..=1",
                equipment.id
            );
        }
    }

    for enemy in &ENEMIES {
        assert!(
            ENTITIES.contains(enemy.entity),
            "ennemi `{}` référence l'entité inconnue `{}`",
            enemy.id,
            enemy.entity
        );

        if let Some(weapon) = enemy.weapon {
            assert!(
                WEAPONS.contains(weapon),
                "ennemi `{}` référence l'arme inconnue `{weapon}`",
                enemy.id
            );
        }

        for loot in enemy.loot {
            assert!(
                ITEMS.contains(loot.item),
                "ennemi `{}` : butin vers l'item inconnu `{}`",
                enemy.id,
                loot.item
            );
            assert!(
                (0.0..=1.0).contains(&loot.chance),
                "ennemi `{}` : chance de butin hors de 0..=1 pour `{}`",
                enemy.id,
                loot.item
            );
            assert!(
                loot.min <= loot.max,
                "ennemi `{}` : quantité de butin inversée pour `{}`",
                enemy.id,
                loot.item
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_consistent() {
        validate();
    }

    #[test]
    fn unknown_id_is_none() {
        assert!(ITEMS.get(Id("n_existe_pas")).is_none());
    }

    #[test]
    fn weapon_and_item_are_linked_both_ways() {
        for weapon in &WEAPONS {
            let item = ITEMS.expect(weapon.item);
            assert_eq!(
                item.kind,
                ItemKind::Weapon(weapon.id),
                "l'item `{}` ne pointe pas vers l'arme `{}`",
                item.id,
                weapon.id
            );
            assert_eq!(item.max_stack, 1, "une arme ne s'empile pas : `{}`", item.id);
        }
    }

    #[test]
    fn equipment_and_item_are_linked_both_ways() {
        for equipment in &EQUIPMENTS {
            let item = ITEMS.expect(equipment.item);
            assert_eq!(
                item.kind,
                ItemKind::Equipment(equipment.id),
                "l'item `{}` ne pointe pas vers l'équipement `{}`",
                item.id,
                equipment.id
            );
        }
    }

    #[test]
    fn stats_sum_to_neutral_element() {
        let total: Stats = EQUIPMENTS.iter().map(|e| e.stats).sum();
        assert_eq!(total.plus(Stats::NONE), total);
    }
}
