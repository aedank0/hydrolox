use std::{
    collections::hash_map::{self, Entry},
    fmt::{Debug, Display},
    iter::{FusedIterator, Iterator, Zip},
    marker::PhantomData,
    num::NonZeroU64,
    sync::RwLock,
    vec,
};

use ahash::AHashMap;
use serde::{
    de::{Unexpected, Visitor},
    ser::SerializeMap,
    Deserialize, Serialize,
};

use crate::{game, physics, render};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Entity {
    id: NonZeroU64,
}
mod entity_impl {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    pub fn next_id() -> u64 {
        NEXT_ID.fetch_add(1, Ordering::SeqCst)
    }
}
impl Entity {
    pub const RESERVED: Self = Self {
        id: NonZeroU64::MAX,
    };
    pub fn new() -> Self {
        Self {
            id: NonZeroU64::new(entity_impl::next_id()).unwrap(),
        }
    }
}
impl Serialize for Entity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.id.get())
    }
}
impl<'de> Deserialize<'de> for Entity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EntityVisitor;
        impl<'de> Visitor<'de> for EntityVisitor {
            type Value = Entity;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a positive integer <= 2^64")
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Entity {
                    id: NonZeroU64::new(v).ok_or(serde::de::Error::invalid_value(
                        Unexpected::Unsigned(v),
                        &"a non-zero value",
                    ))?,
                })
            }
            fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u64(v as u64)
            }
            fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u64(v as u64)
            }
            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_u64(v as u64)
            }
            fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v > u64::MAX as u128 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Other("integer greater than 2^64"),
                        &"a non-zero integer <= 2^64",
                    ))
                } else {
                    self.visit_u64(v as u64)
                }
            }
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v < 0 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Signed(v),
                        &"a positive non-zero integer",
                    ))
                } else {
                    self.visit_u64(v as u64)
                }
            }
            fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v < 0 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Signed(v as i64),
                        &"a positive non-zero integer",
                    ))
                } else {
                    self.visit_u64(v as u64)
                }
            }
            fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v < 0 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Signed(v as i64),
                        &"a positive non-zero integer",
                    ))
                } else {
                    self.visit_u64(v as u64)
                }
            }
            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v < 0 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Signed(v as i64),
                        &"a positive non-zero integer",
                    ))
                } else {
                    self.visit_u64(v as u64)
                }
            }
            fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v < 0 {
                    Err(serde::de::Error::invalid_value(
                        Unexpected::Other("negative integer"),
                        &"a positive non-zero integer",
                    ))
                } else {
                    self.visit_u128(v as u128)
                }
            }
        }

        deserializer.deserialize_u64(EntityVisitor)
    }
}
impl Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

pub trait Component: 'static + Debug {}

#[derive(Debug, Clone)]
pub struct CompIter<'a, T> {
    comps: &'a Vec<T>,
    id_iter: hash_map::Iter<'a, Entity, usize>,
}
impl<'a, T> CompIter<'a, T>
where
    T: Component,
{
    fn new(components: &'a Comptainer<T>) -> Self {
        Self {
            comps: &components.comps,
            id_iter: components.id_to_pos.iter(),
        }
    }
}
impl<'a, T> Iterator for CompIter<'a, T>
where
    T: Component,
{
    type Item = (Entity, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((&e, &i)) = self.id_iter.next() {
            Some((e, &self.comps[i]))
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.comps.len();
        (len, Some(len))
    }
}
impl<T> ExactSizeIterator for CompIter<'_, T> where T: Component {}
impl<T> FusedIterator for CompIter<'_, T> where T: Component {}

#[derive(Debug)]
pub struct CompIterMut<'a, T> {
    comps: &'a mut Vec<T>,
    id_iter: hash_map::Iter<'a, Entity, usize>,
}
impl<'a, T> CompIterMut<'a, T>
where
    T: Component,
{
    fn new(components: &'a mut Comptainer<T>) -> Self {
        Self {
            comps: &mut components.comps,
            id_iter: components.id_to_pos.iter(),
        }
    }
}
impl<'a, T> Iterator for CompIterMut<'a, T>
where
    T: Component,
{
    type Item = (Entity, &'a mut T);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((&e, &i)) = self.id_iter.next() {
            // Unsafe code is the only way to do this.
            // Is safe because a reference to the same component won't be returned twice, so data won't be aliased.
            Some((e, unsafe { &mut *self.comps.as_mut_ptr().add(i) }))
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.comps.len();
        (len, Some(len))
    }
}
impl<T> ExactSizeIterator for CompIterMut<'_, T> where T: Component {}
impl<T> FusedIterator for CompIterMut<'_, T> where T: Component {}

#[derive(Debug)]
pub struct Comptainer<T> {
    id_to_pos: AHashMap<Entity, usize>,
    comps: Vec<T>,
}
impl<T> Default for Comptainer<T>
where
    T: Component,
{
    fn default() -> Self {
        Self {
            id_to_pos: AHashMap::default(),
            comps: Vec::default(),
        }
    }
}
impl<T> Comptainer<T>
where
    T: Component,
{
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            id_to_pos: AHashMap::with_capacity(cap),
            comps: Vec::with_capacity(cap),
        }
    }
    pub fn has_component(&self, entity: Entity) -> bool {
        self.id_to_pos.contains_key(&entity)
    }
    pub fn add_component(&mut self, entity: Entity, component: T) -> Option<T> {
        match self.id_to_pos.entry(entity) {
            Entry::Occupied(entry) => {
                Some(std::mem::replace(&mut self.comps[*entry.get()], component))
            }
            Entry::Vacant(entry) => {
                entry.insert(self.comps.len());
                self.comps.push(component);
                None
            }
        }
    }
    pub fn remove_component(&mut self, entity: Entity) -> bool {
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
    pub fn len(&self) -> usize {
        self.id_to_pos.len()
    }
    pub fn take(&mut self) -> (Vec<Entity>, Vec<T>) {
        let mut entities = vec![Entity::RESERVED; self.id_to_pos.len()];
        std::mem::take(&mut self.id_to_pos)
            .into_iter()
            .for_each(|(e, pos)| entities[pos] = e);

        (entities, std::mem::take(&mut self.comps))
    }
    pub fn take_iter(&mut self) -> Zip<vec::IntoIter<Entity>, vec::IntoIter<T>> {
        let (e, c) = self.take();
        e.into_iter().zip(c.into_iter())
    }
    pub fn iter(&self) -> CompIter<T> {
        CompIter::new(self)
    }
    pub fn iter_mut(&mut self) -> CompIterMut<T> {
        CompIterMut::new(self)
    }
    pub fn get(&self, entity: Entity) -> Option<&T> {
        if let Some(&i) = self.id_to_pos.get(&entity) {
            Some(&self.comps[i])
        } else {
            None
        }
    }
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        if let Some(&i) = self.id_to_pos.get(&entity) {
            Some(&mut self.comps[i])
        } else {
            None
        }
    }
    pub fn get_one(&self) -> Option<(Entity, &T)> {
        if let Some((e, i)) = self.id_to_pos.iter().next() {
            Some((*e, &self.comps[*i]))
        } else {
            None
        }
    }
}
impl<T> Serialize for Comptainer<T>
where
    T: Component + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (e, idx) in &self.id_to_pos {
            map.serialize_entry(e, &self.comps[*idx])?;
        }
        map.end()
    }
}
impl<'de, T> Deserialize<'de> for Comptainer<T>
where
    T: Component + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ComptainerVisitor<T> {
            marker: PhantomData<fn() -> Comptainer<T>>,
        }
        impl<T> ComptainerVisitor<T>
        where
            T: Component,
        {
            fn new() -> Self {
                Self {
                    marker: PhantomData,
                }
            }
        }
        impl<'de, T> Visitor<'de> for ComptainerVisitor<T>
        where
            T: Component + Deserialize<'de>,
        {
            type Value = Comptainer<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("map of entity ids to components")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut comptainer = Comptainer::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((key, value)) = map.next_entry()? {
                    _ = comptainer.add_component(key, value);
                }

                Ok(comptainer)
            }
        }
        deserializer.deserialize_map(ComptainerVisitor::new())
    }
}

#[derive(Debug, Default)]
pub struct Components {
    pub transforms: RwLock<Comptainer<game::Transform>>,
    pub static_mesh_instances: RwLock<Comptainer<render::StaticMeshInstance>>,
    pub cameras: RwLock<Comptainer<render::Camera>>,
    pub action_handlers: RwLock<Comptainer<game::ActionHandler>>,
    pub physics_bodies: RwLock<Comptainer<physics::PhysicsBody>>,
    pub collision_shapes: RwLock<Comptainer<physics::ColliderShape>>,
    pub uis: RwLock<Comptainer<game::UIComponent>>,
}
impl Components {
    pub fn new() -> Self {
        Self::default()
    }
}
