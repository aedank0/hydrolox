use std::{
    any::{Any, TypeId},
    collections::hash_map::Entry,
    sync::{Arc, RwLock},
    usize,
};

use ahash::AHashMap;

use crate::comp_data::CompData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Entity {
    id: u64,
}
mod entity_impl {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    pub fn next_id() -> u64 {
        NEXT_ID.fetch_add(1, Ordering::SeqCst)
    }
}
impl Entity {
    pub fn new() -> Self {
        Self {
            id: entity_impl::next_id(),
        }
    }
}

#[derive(Debug)]
struct Comptainer {
    id_to_pos: AHashMap<Entity, usize>,
    comps: CompData,
}
impl Comptainer {
    fn new<T: Any>() -> Self {
        Self {
            id_to_pos: AHashMap::default(),
            comps: CompData::new::<T>(),
        }
    }
    fn has_component(&self, entity: Entity) -> bool {
        self.id_to_pos.contains_key(&entity)
    }
    fn add_component<T: Any>(&mut self, entity: Entity, component: T) -> Option<T> {
        match self.id_to_pos.entry(entity) {
            Entry::Occupied(entry) => Some(self.comps.replace(*entry.get(), component)),
            Entry::Vacant(entry) => {
                entry.insert(self.comps.len());
                self.comps.push(component);
                None
            }
        }
    }
    fn remove_component(&mut self, entity: Entity) -> bool {
        if let Some(&i) = self.id_to_pos.get(&entity) {
            self.comps.swap_remove(i);
            if i < self.comps.len() {
                *self
                    .id_to_pos
                    .iter_mut()
                    .find(|(_, v)| **v == self.comps.len())
                    .unwrap()
                    .1 = i;
            }
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Default)]
pub struct Framework {
    components: AHashMap<TypeId, Arc<RwLock<Comptainer>>>,
}
impl Framework {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}
