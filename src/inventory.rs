use bevy::prelude::*;
use std::collections::HashMap;



pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ItemRegistry::default());
    }
}

/// Global registry of item definitions.
#[derive(Resource, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ItemRegistry {
    pub items: HashMap<String, ItemDef>,
}

/// Definition of an item type.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemDef {
    pub name: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u32,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

fn default_max_stack() -> u32 {
    99
}

/// A slot in an inventory.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ItemSlot {
    pub item_id: String,
    pub count: u32,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Inventory component attached to entities.
#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct Inventory {
    pub slots: Vec<ItemSlot>,
    #[serde(default = "default_max_slots")]
    pub max_slots: usize,
}

fn default_max_slots() -> usize {
    20
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            max_slots: 20,
        }
    }
}

impl Inventory {
    pub fn add_item(
        &mut self,
        item_id: &str,
        count: u32,
        registry: &ItemRegistry,
    ) -> u32 {
        let max_stack = registry
            .items
            .get(item_id)
            .map(|d| d.max_stack)
            .unwrap_or(99);

        let mut remaining = count;

        // Try to stack into existing slots
        for slot in &mut self.slots {
            if slot.item_id == item_id && slot.count < max_stack {
                let can_add = (max_stack - slot.count).min(remaining);
                slot.count += can_add;
                remaining -= can_add;
                if remaining == 0 {
                    return count;
                }
            }
        }

        // Add to new slots
        while remaining > 0 && self.slots.len() < self.max_slots {
            let amount = remaining.min(max_stack);
            self.slots.push(ItemSlot {
                item_id: item_id.to_string(),
                count: amount,
                metadata: serde_json::Value::Null,
            });
            remaining -= amount;
        }

        count - remaining
    }

    pub fn remove_item(&mut self, item_id: &str, count: u32) -> u32 {
        let mut remaining = count;
        // Remove from slots in reverse order
        for i in (0..self.slots.len()).rev() {
            if self.slots[i].item_id == item_id {
                let can_remove = self.slots[i].count.min(remaining);
                self.slots[i].count -= can_remove;
                remaining -= can_remove;
                if self.slots[i].count == 0 {
                    self.slots.remove(i);
                }
                if remaining == 0 {
                    break;
                }
            }
        }
        count - remaining
    }

    pub fn has_item(&self, item_id: &str) -> bool {
        self.slots.iter().any(|s| s.item_id == item_id && s.count > 0)
    }

    pub fn count_item(&self, item_id: &str) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.item_id == item_id)
            .map(|s| s.count)
            .sum()
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }
}
