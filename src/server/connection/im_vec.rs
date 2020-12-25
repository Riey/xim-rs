use ahash::AHashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroU16;

pub struct ImVec<T> {
    next: NonZeroU16,
    inner: AHashMap<NonZeroU16, T>,
}

impl<T> ImVec<T> {
    pub fn new() -> Self {
        Self {
            next: NonZeroU16::new(1).unwrap(),
            inner: AHashMap::new(),
        }
    }

    fn next(&mut self) -> NonZeroU16 {
        let ret = self.next;
        self.next = NonZeroU16::new(self.next.get() + 1).unwrap();
        ret
    }

    pub fn new_item(&mut self, data: T) -> (NonZeroU16, &mut T) {
        let idx = self.next();

        let val = match self.inner.entry(idx) {
            Entry::Occupied(mut o) => {
                o.insert(data);
                o.into_mut()
            }
            Entry::Vacant(v) => v.insert(data),
        };

        (idx, val)
    }

    #[allow(unused)]
    pub fn remove_item(&mut self, idx: NonZeroU16) -> Option<T> {
        self.inner.remove(&idx)
    }

    pub fn get_item(&mut self, idx: NonZeroU16) -> Option<&mut T> {
        self.inner.get_mut(&idx)
    }
}
