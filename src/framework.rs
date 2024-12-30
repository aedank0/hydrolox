use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    any::{Any, TypeId},
    collections::hash_map::Entry,
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
    ops::Deref,
    ptr::{copy_nonoverlapping, NonNull},
    sync::{Arc, RwLock},
    usize,
};

use ahash::AHashMap;

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

fn safe_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, align).unwrap();
    let ptr = unsafe { alloc(layout.clone()) };
    if ptr.is_null() {
        handle_alloc_error(layout);
    }
    ptr
}

fn safe_ty_alloc<T>(size: usize, align: usize) -> *mut T {
    safe_alloc(size, align) as *mut T
}

fn safe_arr_alloc<T>(cap: usize) -> *mut T {
    safe_ty_alloc(cap * size_of::<T>(), align_of::<T>())
}

#[derive(Debug)]
struct CompData {
    data: Option<NonNull<dyn Any>>,
    len: usize,
    cap: usize,
    elem_type: TypeId,
    elem_size: usize,
    elem_align: usize,
}
impl CompData {
    fn new<T: Any>() -> Self {
        Self::with_capacity::<T>(4)
    }
    fn with_capacity<T: Any>(cap: usize) -> Self {
        match size_of::<T>() {
            0 => Self {
                data: None,
                len: 0,
                cap: usize::MAX,
                elem_type: TypeId::of::<T>(),
                elem_size: size_of::<T>(),
                elem_align: align_of::<T>(),
            },
            _ => Self {
                data: Some(NonNull::new(safe_arr_alloc::<T>(cap)).unwrap()),
                len: 0,
                cap,
                elem_type: TypeId::of::<T>(),
                elem_size: size_of::<T>(),
                elem_align: align_of::<T>(),
            },
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn grow(&mut self) {
        let ptr = self.data.unwrap().as_ptr() as *mut u8;
        let new_cap = self.cap * 2;
        let new_data = NonNull::new(safe_alloc(new_cap, self.elem_align)).unwrap();
        unsafe {
            copy_nonoverlapping(ptr, new_data.as_ptr() as *mut u8, self.len * self.elem_size);
            dealloc(
                ptr,
                Layout::from_size_align(self.cap * self.elem_size, self.elem_align).unwrap(),
            );
        }

        self.data = Some(new_data);
        self.cap = new_cap;
    }
    fn push<T: Any>(&mut self, val: T) {
        assert_eq!(self.elem_type, TypeId::of::<T>());

        if let Some(ptr) = self.data {
            if self.len == self.cap {
                self.grow();
            }

            unsafe {
                ptr.cast::<T>().add(self.len).write(val);
            }
        }

        self.len += 1;
    }
    fn replace<T: Any>(&mut self, i: usize, val: T) -> T {
        assert_eq!(self.elem_type, TypeId::of::<T>());
        assert!(i < self.len);
        if let Some(ptr) = self.data {
            unsafe { ptr.cast::<T>().add(i).replace(val) }
        } else {
            val
        }
    }
    fn swap_remove(&mut self, i: usize) {
        assert!(i < self.len);
        if let Some(ptr) = self.data {
            let to_remove = unsafe { ptr.byte_add(i * self.elem_size) };
            unsafe {
                to_remove.drop_in_place();
            }
            self.len -= 1;
            if i < self.len {
                unsafe {
                    copy_nonoverlapping(
                        ptr.byte_add(self.len * self.elem_size)
                            .cast::<u8>()
                            .as_ptr(),
                        to_remove.cast().as_ptr(),
                        self.elem_size,
                    )
                }
            }
        }
    }
}
impl Drop for CompData {
    fn drop(&mut self) {
        if let Some(ptr) = self.data {
            for i in 0..self.len {
                unsafe {
                    ptr.byte_add(i * self.elem_size).drop_in_place();
                }
            }
            unsafe {
                dealloc(
                    ptr.cast().as_ptr(),
                    Layout::from_size_align(self.cap * self.elem_size, self.elem_align).unwrap(),
                );
            }
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
