//! Items : tout ce qui peut occuper une case d'inventaire.
//!
//! Une arme ou un équipement possède **aussi** une entrée ici : l'item porte ce
//! qui est commun (nom, rareté, icône, empilement), la table spécialisée porte
//! le comportement. Le lien se fait par [`ItemKind`].

use super::common::Rarity;
use super::registry::{Definition, Id, Registry};

/// Rôle de l'item, et lien éventuel vers sa table spécialisée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    /// Ressource de fabrication, sans effet propre.
    Material,
    /// Consommable : un usage, puis l'item est retiré de l'inventaire.
    Consumable(ConsumableEffect),
    /// Renvoie vers [`super::weapons::WEAPONS`].
    Weapon(Id),
    /// Renvoie vers [`super::equipments::EQUIPMENTS`].
    Equipment(Id),
}

/// Effet d'un consommable à l'utilisation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsumableEffect {
    /// Rend des points de vie immédiatement.
    Heal { amount: f32 },
    /// Modifie la vitesse pendant une durée, en secondes.
    Haste { bonus: f32, duration: f32 },
}

/// Définition statique d'un item.
#[derive(Debug, Clone, Copy)]
pub struct ItemDef {
    pub id: Id,
    /// Nom affiché.
    pub name: &'static str,
    /// Texte d'ambiance / description courte.
    pub description: &'static str,
    pub rarity: Rarity,
    /// Nombre maximum d'exemplaires par case d'inventaire (≥ 1).
    pub max_stack: u16,
    /// Chemin de l'icône, relatif à `assets/`.
    pub icon: &'static str,
    pub kind: ItemKind,
}

impl Definition for ItemDef {
    const KIND: &'static str = "item";

    fn id(&self) -> Id {
        self.id
    }
}

impl ItemDef {
    pub const fn is_stackable(&self) -> bool {
        self.max_stack > 1
    }
}

/// Table des items du jeu.
pub const ITEMS: Registry<ItemDef> = Registry::new(&[
    ItemDef {
        id: Id("scrap"),
        name: "Ferraille",
        description: "Des morceaux de métal encore utilisables.",
        rarity: Rarity::Common,
        max_stack: 99,
        icon: "items/scrap.png",
        kind: ItemKind::Material,
    },
    ItemDef {
        id: Id("energy_cell"),
        name: "Cellule d'énergie",
        description: "Une batterie compacte, chaude au toucher.",
        rarity: Rarity::Uncommon,
        max_stack: 20,
        icon: "items/energy_cell.png",
        kind: ItemKind::Material,
    },
    ItemDef {
        id: Id("medkit"),
        name: "Trousse de soin",
        description: "Referme les plaies, pas les rancunes.",
        rarity: Rarity::Common,
        max_stack: 5,
        icon: "items/medkit.png",
        kind: ItemKind::Consumable(ConsumableEffect::Heal { amount: 40.0 }),
    },
    ItemDef {
        id: Id("stim"),
        name: "Stimulant",
        description: "Le monde ralentit. Vous, non.",
        rarity: Rarity::Rare,
        max_stack: 3,
        icon: "items/stim.png",
        kind: ItemKind::Consumable(ConsumableEffect::Haste {
            bonus: 120.0,
            duration: 6.0,
        }),
    },
    ItemDef {
        id: Id("pistol"),
        name: "Pistolet",
        description: "Fiable, sans plus.",
        rarity: Rarity::Common,
        max_stack: 1,
        icon: "items/pistol.png",
        kind: ItemKind::Weapon(Id("pistol")),
    },
    ItemDef {
        id: Id("shotgun"),
        name: "Fusil à pompe",
        description: "Conversation courte, argument large.",
        rarity: Rarity::Uncommon,
        max_stack: 1,
        icon: "items/shotgun.png",
        kind: ItemKind::Weapon(Id("shotgun")),
    },
    ItemDef {
        id: Id("arc_rifle"),
        name: "Fusil à arc",
        description: "L'orage tient dans les mains.",
        rarity: Rarity::Epic,
        max_stack: 1,
        icon: "items/arc_rifle.png",
        kind: ItemKind::Weapon(Id("arc_rifle")),
    },
    ItemDef {
        id: Id("scrap_helmet"),
        name: "Casque de ferraille",
        description: "Bricolé, mais il tient.",
        rarity: Rarity::Common,
        max_stack: 1,
        icon: "items/scrap_helmet.png",
        kind: ItemKind::Equipment(Id("scrap_helmet")),
    },
    ItemDef {
        id: Id("plated_vest"),
        name: "Gilet plaqué",
        description: "Lourd là où il faut.",
        rarity: Rarity::Uncommon,
        max_stack: 1,
        icon: "items/plated_vest.png",
        kind: ItemKind::Equipment(Id("plated_vest")),
    },
    ItemDef {
        id: Id("runner_boots"),
        name: "Bottes de coureur",
        description: "Usées jusqu'à la semelle, et pourtant rapides.",
        rarity: Rarity::Rare,
        max_stack: 1,
        icon: "items/runner_boots.png",
        kind: ItemKind::Equipment(Id("runner_boots")),
    },
]);
