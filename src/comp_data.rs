use std::{
    alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout},
    any::{Any, TypeId},
    ptr::NonNull,
    usize,
};

fn alloc_nonnull<T>(count: usize) -> NonNull<T> {
    let layout = Layout::from_size_align(size_of::<T>() * count, align_of::<T>()).unwrap();
    let ptr = unsafe { alloc(layout.clone()) };
    if let Some(nonnull) = NonNull::new(ptr) {
        nonnull.cast()
    } else {
        handle_alloc_error(layout)
    }
}

fn realloc_nonnull<T>(ptr: NonNull<T>, old_count: usize, new_count: usize) -> Option<NonNull<T>> {
    let layout = Layout::from_size_align(old_count * size_of::<T>(), align_of::<T>()).unwrap();
    let ptr = unsafe { realloc(ptr.cast().as_ptr(), layout, new_count * size_of::<T>()) };
    NonNull::new(ptr as *mut T)
}

fn dealloc_nonnull<T>(ptr: NonNull<T>, count: usize) {
    let layout = Layout::from_size_align(count * size_of::<T>(), align_of::<T>()).unwrap();
    unsafe {
        dealloc(ptr.cast().as_ptr(), layout);
    }
}

#[derive(Debug)]
pub struct CompData {
    data: Option<NonNull<dyn Any>>,
    len: usize,
    cap: usize,

    elem_type: TypeId,
    elem_size: usize,
    elem_align: usize,
}
impl CompData {
    pub fn new<T: Any>() -> Self {
        Self::with_capacity::<T>(0)
    }
    pub fn with_capacity<T: Any>(cap: usize) -> Self {
        if size_of::<T>() == 0 {
            Self {
                data: None,
                len: 0,
                cap: usize::MAX,
                elem_type: TypeId::of::<T>(),
                elem_size: size_of::<T>(),
                elem_align: align_of::<T>(),
            }
        } else {
            if cap == 0 {
                Self {
                    data: None,
                    len: 0,
                    cap: 0,
                    elem_type: TypeId::of::<T>(),
                    elem_size: size_of::<T>(),
                    elem_align: align_of::<T>(),
                }
            } else {
                Self {
                    data: Some(alloc_nonnull::<T>(cap)),
                    len: 0,
                    cap,
                    elem_type: TypeId::of::<T>(),
                    elem_size: size_of::<T>(),
                    elem_align: align_of::<T>(),
                }
            }
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    fn grow<T: Any>(&mut self) {
        assert!(self.elem_size > 0);

        if let Some(data) = self.data {
            let new_cap = self.cap + self.cap / 2;
            if let Some(realloc) = realloc_nonnull::<T>(data.cast(), self.cap, new_cap) {
                self.data.insert(realloc);
            } else {
                let new_data = alloc_nonnull::<T>(new_cap);
                unsafe {
                    let old_data: NonNull<T> = data.cast();
                    old_data.copy_to_nonoverlapping(new_data, self.len);
                    dealloc_nonnull(old_data, self.cap);
                }
            }
            self.cap = new_cap;
        } else {
            self.cap = 8;
            self.data.insert(alloc_nonnull::<T>(8));
        }
    }
    pub fn push<T: Any>(&mut self, val: T) {
        assert_eq!(self.elem_type, TypeId::of::<T>());

        if size_of::<T>() > 0 {
            if self.data.is_none() || self.len == self.cap {
                self.grow::<T>();
            }
            unsafe {
                self.data.unwrap().cast::<T>().add(self.len).write(val);
            }
        }

        self.len += 1;
    }
    pub fn replace<T: Any>(&mut self, i: usize, val: T) -> T {
        assert_eq!(self.elem_type, TypeId::of::<T>());
        assert!(i < self.len);
        if size_of::<T>() > 0 {
            unsafe { self.data.unwrap().cast::<T>().add(i).replace(val) }
        } else {
            val
        }
    }
    pub fn swap_remove(&mut self, i: usize) {
        assert!(i < self.len);

        let new_len = self.len - 1;

        if self.elem_size > 0 {
            let data = self.data.unwrap();
            unsafe {
                let to_remove = data.byte_add(i * self.elem_size);
                to_remove.drop_in_place();
                if i < new_len {
                    data.byte_add(new_len * self.elem_size)
                        .cast::<u8>()
                        .copy_to_nonoverlapping(to_remove.cast(), self.elem_size);
                }
            }
        }

        self.len = new_len;
    }
    pub fn as_typed_slice<T: Any>(&self) -> &[T] {
        assert_eq!(self.elem_type, TypeId::of::<T>());
        if let Some(data) = self.data {
            unsafe { NonNull::slice_from_raw_parts(data.cast(), self.len).as_ref() }
        } else {
            unsafe { NonNull::slice_from_raw_parts(NonNull::<T>::dangling(), self.len).as_ref() }
        }
    }
    pub fn as_typed_slice_mut<T: Any>(&mut self) -> &mut [T] {
        assert_eq!(self.elem_type, TypeId::of::<T>());
        if let Some(data) = self.data {
            unsafe { NonNull::slice_from_raw_parts(data.cast(), self.len).as_mut() }
        } else {
            unsafe { NonNull::slice_from_raw_parts(NonNull::<T>::dangling(), self.len).as_mut() }
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
