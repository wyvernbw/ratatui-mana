use std::alloc;
use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::{any::Any, ptr::NonNull};

use crate::layout::{ElWidget, NopWidget};

/// size: 56 bytes
/// this specialization is required as rust has no way to pass trait bounds
/// as a generic.
#[derive(derive_more::Debug)]
struct VecWidgets {
    #[debug(skip)]
    buf: NonNull<u8>,
    capacity: usize,
    len: usize,
    meta: VecUnsizedMetadata,
}

#[derive(derive_more::Debug)]
struct VecUnsizedMetadata {
    #[debug(skip)]
    drop_impl: fn(&mut dyn Any),
    #[debug(skip)]
    extract: unsafe fn(*mut u8) -> *mut dyn ElWidget,
    layout: alloc::Layout,
}

unsafe impl Send for VecWidgets {}

impl VecUnsizedMetadata {
    fn drop_any<T: 'static>(value: &mut dyn Any) {
        unsafe {
            debug_assert!(value.is::<T>());
            let value = value.downcast_unchecked_mut::<T>();
            std::ptr::drop_in_place(value);
        }
    }
    fn new<T: ElWidget + 'static>() -> Self {
        Self {
            drop_impl: Self::drop_any::<T>,
            extract: |ptr| unsafe { ptr.cast::<T>().as_mut_unchecked() as &mut dyn ElWidget },
            layout: alloc::Layout::new::<T>(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TryReserveError<T> {
    #[error("failed to allocate buffer for VecUnsized")]
    AllocError,
    #[error(
        "attempt to reserve with different layout: type {typename} has layout {0:?}",
        typename = std::any::type_name::<T>()
    )]
    LayoutError(alloc::Layout, PhantomData<T>),
}

impl VecWidgets {
    const INITIAL_CAPACITY: usize = 8;
    fn new() -> Self {
        VecWidgets {
            buf: NonNull::dangling(),
            capacity: 0,
            len: 0,
            meta: VecUnsizedMetadata::new::<NopWidget>(),
        }
    }
    fn with_capacity<T: ElWidget + 'static>(n: usize) -> Self {
        let mut vec = Self::new();
        vec.reserve_exact::<T>(n);
        vec
    }
    fn create_buffer_layout(&self, n: usize) -> alloc::Layout {
        self.meta.layout.repeat(n).unwrap().0
    }
    fn try_reserve_exact<T: ElWidget + 'static>(
        &mut self,
        new_capacity: usize,
    ) -> Result<(), TryReserveError<T>> {
        if new_capacity <= self.capacity {
            return Ok(());
        }
        match self.capacity {
            0 => {
                let meta = VecUnsizedMetadata::new::<T>();
                self.capacity = new_capacity;
                self.meta = meta;
                self.len = 0;
                let buf = unsafe { std::alloc::alloc(self.create_buffer_layout(self.capacity)) };
                let buf = NonNull::new(buf).ok_or(TryReserveError::AllocError)?;
                self.buf = buf;
                Ok(())
            }
            _ => {
                let layout = alloc::Layout::new::<T>();
                if self.meta.layout != layout {
                    return Err(TryReserveError::LayoutError(layout, PhantomData));
                }
                unsafe {
                    let buf = alloc::realloc(
                        self.buf.as_ptr(),
                        self.create_buffer_layout(self.capacity),
                        self.create_buffer_layout(new_capacity).size(),
                    );
                    self.buf = NonNull::new(buf).ok_or(TryReserveError::AllocError)?;
                    self.capacity = new_capacity;
                }
                Ok(())
            }
        }
    }
    fn reserve_exact<T: ElWidget + 'static>(&mut self, new_capacity: usize) {
        if let Err(err) = self.try_reserve_exact::<T>(new_capacity) {
            panic!(
                "VecUnsized::reserve_exact::<{}>: {err}",
                std::any::type_name::<T>()
            )
        }
    }
    fn try_reserve<T: ElWidget + 'static>(
        &mut self,
        new_capacity: usize,
    ) -> Result<(), TryReserveError<T>> {
        if new_capacity <= self.capacity {
            return Ok(());
        }
        let new_capacity = (self.capacity * 2).max(new_capacity);
        self.try_reserve_exact(new_capacity)
    }
    fn reserve<T: ElWidget + 'static>(&mut self, new_capacity: usize) {
        if let Err(err) = self.try_reserve::<T>(new_capacity) {
            panic!(
                "VecUnsized::reserve::<{}>: {err}",
                std::any::type_name::<T>()
            )
        }
    }
    fn put_back<T: 'static>(&mut self, value: T) -> &mut T {
        assert!(self.meta.layout == alloc::Layout::new::<T>());
        unsafe {
            self.buf.as_ptr().cast::<T>().add(self.len).write(value);
        }
        self.len += 1;
        let len = self.len;
        &mut self.as_slice_mut()[len - 1]
    }
    fn push<T>(&mut self, value: T) -> &mut T
    where
        T: Send + 'static,
        T: Any + ElWidget,
    {
        self.reserve::<T>(self.len + 1);
        self.put_back(value)
    }

    fn as_slice<T: 'static>(&self) -> &[T] {
        assert!(self.meta.layout == alloc::Layout::new::<T>());
        unsafe { std::slice::from_raw_parts(self.buf.cast::<T>().as_ptr(), self.len) }
    }

    fn as_slice_mut<T: 'static>(&mut self) -> &mut [T] {
        assert!(self.meta.layout == alloc::Layout::new::<T>());
        unsafe { std::slice::from_raw_parts_mut(self.buf.cast::<T>().as_ptr(), self.len) }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn get<T: 'static>(&self, idx: usize) -> &T {
        &self.as_slice()[idx]
    }

    fn try_get<T: 'static>(&self, idx: usize) -> Option<&T> {
        self.as_slice().get(idx)
    }

    fn get_mut<T: 'static>(&mut self, idx: usize) -> &mut T {
        &mut self.as_slice_mut()[idx]
    }

    fn try_get_mut<T: 'static>(&mut self, idx: usize) -> Option<&mut T> {
        self.as_slice_mut().get_mut(idx)
    }

    fn get_widget(&self, idx: usize) -> &(dyn ElWidget + 'static) {
        assert!(idx < self.len);
        unsafe {
            let to_any = self.meta.extract;
            let ptr = self.buf.add(idx * self.meta.layout.pad_to_align().size());

            to_any(ptr.as_ptr()).as_ref_unchecked()
        }
    }
    fn get_widget_mut(&mut self, idx: usize) -> &mut (dyn ElWidget + 'static) {
        assert!(idx < self.len);
        unsafe {
            let to_any = self.meta.extract;
            let ptr = self.buf.add(idx);

            to_any(ptr.as_ptr()).as_mut_unchecked()
        }
    }
}

struct Iter<'a, T> {
    vec: &'a VecWidgets,
    endptr: NonNull<u8>,
    cursor: NonNull<u8>,
    _ty: PhantomData<T>,
}

impl VecWidgets {
    fn iter<T: 'static>(&self) -> Iter<'_, T> {
        let el_layout = alloc::Layout::new::<T>();
        assert!(
            self.meta.layout == el_layout,
            "VecUnsized: tried to iterate over elements of a different layout than the one contained: expected {:?}, got {:?}",
            self.meta.layout,
            el_layout
        );
        let meta = &self.meta;
        let layout = meta.layout.pad_to_align();
        let endptr = unsafe { self.buf.add(layout.size() * self.len) };
        let cursor = self.buf;
        Iter {
            vec: self,
            endptr,
            cursor,
            _ty: PhantomData,
        }
    }
}

impl<'a, T: 'static> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let layout = self.vec.meta.layout.pad_to_align();
        if self.cursor < self.endptr {
            // SAFETY: checked at Iter creation time
            unsafe {
                let any = (self.cursor.as_ptr() as *mut dyn Any).as_mut_unchecked();
                let value = any.downcast_unchecked_ref::<T>();
                self.cursor = self.cursor.add(layout.size());
                Some(value)
            }
        } else {
            None
        }
    }
}

struct IterAnyMut<'a> {
    vec: &'a mut VecWidgets,
    state: IterState<'a>,
}

struct IterState<'a> {
    endptr: NonNull<u8>,
    cursor: NonNull<u8>,
    _marker: PhantomData<&'a ()>,
}

impl VecWidgets {
    fn iter_any_mut(&mut self) -> IterAnyMut<'_> {
        let meta = &self.meta;
        let layout = meta.layout.pad_to_align();
        let endptr = unsafe { self.buf.add(layout.size() * self.len) };
        let cursor = self.buf;
        IterAnyMut {
            vec: self,
            state: IterState {
                endptr,
                cursor,
                _marker: PhantomData,
            },
        }
    }
}

impl<'a> Iterator for IterAnyMut<'a> {
    type Item = &'a mut dyn Any;

    fn next(&mut self) -> Option<Self::Item> {
        let layout = self.vec.meta.layout.pad_to_align();
        if self.state.cursor < self.state.endptr {
            // SAFETY: checked at IterAnyMut creation time
            unsafe {
                let cursor = self.state.cursor;
                self.state.cursor = self.state.cursor.add(layout.size());
                let to_any = self.vec.meta.extract;
                let any = to_any(cursor.as_ptr()).as_mut_unchecked();
                Some(any)
            }
        } else {
            None
        }
    }
}

impl Default for VecWidgets {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VecWidgets {
    fn drop(&mut self) {
        let meta = &self.meta;
        let drop_impl = meta.drop_impl;
        for value in self.iter_any_mut() {
            drop_impl(value);
        }
        if self.capacity > 0 {
            unsafe {
                alloc::dealloc(self.buf.as_ptr(), self.create_buffer_layout(self.capacity));
            }
        }
    }
}

/// implementation inspired by `slotmap`'s [`SlotMap`].
/// [https://docs.rs/slotmap/latest/src/slotmap/basic.rs.html#129-134]
#[derive(Debug)]
pub(crate) struct TypeArena {
    slots: VecWidgets,
    count: usize,
    next_free: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeKey {
    index: usize,
    version: usize,
    pub(crate) typeid: TypeId,
}

pub(crate) struct TypeSlot<V> {
    u: TypeSlotUnion<V>,
    version: usize,
}

union TypeSlotUnion<V> {
    occupied: ManuallyDrop<OccupiedSlot<V>>,
    vacant: ManuallyDrop<VacantSlot>,
}

impl<V> std::fmt::Debug for TypeSlot<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeSlot")
            .field("occupied", &self.occupied())
            .field("version", &self.version)
            .finish()
    }
}

impl<V> TypeSlot<V> {
    fn occupied(&self) -> bool {
        !self.version.is_multiple_of(2)
    }
    fn vacant(&self) -> bool {
        self.version.is_multiple_of(2)
    }
}

impl<V> ElWidget for TypeSlot<V>
where
    V: Send + ElWidget + 'static,
{
    fn render_element(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        unsafe {
            self.u.occupied.value.render_element(area, buf);
        }
    }

    fn key_version(&self) -> usize {
        self.version
    }
}

#[derive(Debug, Clone)]
struct OccupiedSlot<V> {
    value: V,
}

#[derive(Debug, Clone)]
struct VacantSlot {
    next_free: usize,
}

impl TypeArena {
    pub(crate) fn with_capacity<T>(n: usize) -> Self
    where
        TypeSlot<T>: ElWidget,
    {
        TypeArena {
            slots: VecWidgets::with_capacity::<TypeSlot<T>>(n),
            count: 0,
            next_free: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(crate) fn insert<T>(&mut self, value: T) -> TypeKey
    where
        T: 'static,
        TypeSlot<T>: ElWidget,
    {
        match self.slots.try_get_mut::<TypeSlot<T>>(self.next_free) {
            Some(next_free) => unsafe {
                if next_free.occupied() {
                    self.next_free = self.slots.len();
                    return self.insert(value);
                }

                let idx = self.next_free;
                {
                    let vacant = ManuallyDrop::take(&mut next_free.u.vacant);
                    self.next_free = vacant.next_free;
                    self.count += 1;
                }
                next_free.u.occupied = ManuallyDrop::new(OccupiedSlot { value });
                next_free.version += 2;
                TypeKey {
                    index: idx,
                    version: next_free.version,
                    typeid: TypeId::of::<T>(),
                }
            },
            None => {
                let idx = self.next_free;
                self.slots.push(TypeSlot {
                    version: 1,
                    u: TypeSlotUnion {
                        occupied: ManuallyDrop::new(OccupiedSlot { value }),
                    },
                });
                self.count += 1;
                self.next_free = self.slots.len();
                // tracing::info!(slots = ?self.slots.as_slice::<TypeSlot<T>>());
                TypeKey {
                    typeid: TypeId::of::<T>(),
                    index: idx,
                    version: 1,
                }
            }
        }
    }

    pub(crate) fn remove<T: 'static + Send + Clone>(&mut self, key: TypeKey) -> Option<T> {
        let slot = self.slots.get_mut::<TypeSlot<T>>(key.index);
        if slot.vacant() {
            return None;
        };
        unsafe {
            let occupied = ManuallyDrop::take(&mut slot.u.occupied);
            let value = occupied.value;
            self.count -= 1;
            slot.version += 1;
            slot.u.vacant = ManuallyDrop::new(VacantSlot {
                next_free: self.next_free,
            });
            Some(value)
        }
    }

    pub(crate) fn get<T: 'static + Clone>(&self, key: TypeKey) -> Option<&T> {
        let slot = self.slots.get::<TypeSlot<T>>(key.index);
        if slot.vacant() {
            return None;
        };
        unsafe {
            let occupied = &slot.u.occupied;
            let value = &occupied.value;
            Some(value)
        }
    }

    pub(crate) fn get_widget(&self, key: TypeKey) -> Option<&dyn ElWidget> {
        let widget = self.slots.get_widget(key.index);
        // tracing::info!(?widget);
        let version = widget.key_version();
        if version.is_multiple_of(2) {
            return None;
        };
        Some(widget)
    }
}

pub(crate) type TypeMap = HashMap<TypeId, TypeArena>;
