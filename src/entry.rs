use linked_hash_map;
use std::collections::hash_map::RandomState;
use std::hash::{Hash, BuildHasher};

pub enum Entry<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher = RandomState> {
    Occupied(OccupiedEntry<'a, K, V, S>),
    Vacant(VacantEntry<'a, K, V, S>),
}

impl<'a, K: 'a + Hash + Eq, V: 'a, S: 'a + BuildHasher> Entry<'a, K, V, S> {
    pub fn key(&self) -> &K {
        match self {
            Entry::Occupied(e) => e.key(),
            Entry::Vacant(e) => e.key(),
        }
    }

    pub fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default),
        }
    }

    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default()),
        }
    }
}

pub struct OccupiedEntry<'a, K: 'a, V: 'a, S: 'a = RandomState> {
    pub(crate) entry: linked_hash_map::OccupiedEntry<'a, K, V, S>,
}

impl<'a, K: 'a + Hash + Eq, V: 'a, S: 'a + BuildHasher> OccupiedEntry<'a, K, V, S> {
    /// Gets a reference to the entry key.
    pub fn key(&self) -> &K {
        self.entry.key()
    }

    /// Gets a mutable reference to the value in the entry.
    pub fn get_mut(&mut self) -> &mut V {
        self.entry.get_mut()
    }

    /// Converts the OccupiedEntry into a mutable reference to the value in the
    /// entry with a lifetime bound to the map itself.
    pub fn into_mut(self) -> &'a mut V {
        self.entry.into_mut()
    }

    /// Sets the value of the entry, and returns the entry's old value.
    pub fn insert(&mut self, value: V) -> V {
        self.entry.insert(value)
        // Note: This is an overwrite so we don't need to expire anything.
    }

    /// Takes the value out of the entry, and returns it.
    pub fn remove(self) -> V {
        self.entry.remove()
    }
}

pub struct VacantEntry<'a, K: 'a + Eq + Hash, V: 'a, S: 'a + BuildHasher = RandomState> {
    pub(crate) entry: linked_hash_map::VacantEntry<'a, K, V, S>,

    // This field points to the same cache that the above entry points to. In order to satisfy
    // Rust's lifetime requirements we *must not* turn it into a reference until the above field is
    // dead.
    pub(crate) cache: *mut ::LruCache<K, V, S>,
}

impl<'a, K: 'a + Hash + Eq, V: 'a, S: 'a + BuildHasher> VacantEntry<'a, K, V, S> {
    /// Gets a reference to the entry key.
    pub fn key(&self) -> &K {
        self.entry.key()
    }

    /// Sets the value of the entry with the VacantEntry's key,
    /// and returns a mutable reference to it
    pub fn insert(self, value: V) -> &'a mut V {
        let v = {
            let v: &'a mut V = self.entry.insert(value);

            // Convert to pointer so that we can make a mutable reference to the cache.
            v as *mut V
        };

        // Ideally we would remove before inserting but this requires
        // 1. Knowing that removal won't rehash.
        // 2. Convincing Rust's aliasing rule to play nice.
        //
        // So instead we convert everything to pointers to avoid aliasing
        // assumptions then remove the value.
        {
            let cache = unsafe { &mut*self.cache };
            if cache.len() > cache.capacity() {
                cache.remove_lru();
            }
        }

        unsafe { &mut*v }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_entry_insert() {
        let mut cache = LruCache::new(2);

        {
            let entry = cache.entry(1);
            assert_eq!(entry.key(), &1);
            entry.or_insert(10);
        }
        // Value was inserted and expired 1.
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(&1));

        {
            let entry = cache.entry(2);
            assert_eq!(entry.key(), &2);
            entry.or_insert(20);
        }
        // Value was inserted and expired 1.
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&1));
        assert!(cache.contains_key(&2));

        {
            let entry = cache.entry(3);
            assert_eq!(entry.key(), &3);
            entry.or_insert(30);
        }
        // Value was inserted and expired 1.
        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key(&3));
        assert!(!cache.contains_key(&1));

        {
            let entry = cache.entry(2);
            assert_eq!(entry.key(), &2);
            entry.or_insert(21);
        }
        // Value was already present and didn't insert.
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get_mut(&2), Some(&mut 20));
        assert_eq!(cache.get_mut(&3), Some(&mut 30));

        {
            let entry = cache.entry(4);
            assert_eq!(entry.key(), &4);
            entry.or_insert_with(|| 40);
        }
        // Value was already present and didn't insert.
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get_mut(&3), Some(&mut 30));
        assert_eq!(cache.get_mut(&4), Some(&mut 40));
    }

    #[test]
    fn test_entry_occupied() {
        let mut cache = LruCache::new(2);
        cache.insert(1, 10);
        let old = match cache.entry(1) {
            Entry::Occupied(mut e) => e.insert(11),
            _ => unreachable!("Entry should exist."),
        };
        assert_eq!(old, 10);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get_mut(&1), Some(&mut 11));

        cache.insert(2, 20);
        let old = match cache.entry(2) {
            Entry::Occupied(mut e) => e.insert(21),
            _ => unreachable!("Entry should exist."),
        };
        assert_eq!(old, 20);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get_mut(&1), Some(&mut 11));
        assert_eq!(cache.get_mut(&2), Some(&mut 21));

        let old = match cache.entry(2) {
            Entry::Occupied(mut e) => e.remove(),
            _ => unreachable!("Entry should exist."),
        };
        assert_eq!(old, 21);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get_mut(&1), Some(&mut 11));
    }
}
