//! Bounded LRU reuse for pure derived command data; never part of edit semantics.

use std::collections::HashMap;
use std::hash::Hash;

struct Entry<K, V> {
    key: K,
    value: V,
    previous: Option<usize>,
    next: Option<usize>,
}

pub struct Cache<K, V> {
    capacity: usize,
    lookup: HashMap<K, usize>,
    entries: Vec<Entry<K, V>>,
    first: Option<usize>,
    last: Option<usize>,
}

impl<K: Clone + Eq + Hash, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            capacity,
            lookup: HashMap::new(),
            entries: Vec::new(),
            first: None,
            last: None,
        }
    }

    fn unlink(&mut self, i: usize) {
        let e = &self.entries[i];
        let (previous, next) = (e.previous, e.next);
        if let Some(p) = previous {
            self.entries[p].next = next;
        } else {
            self.first = next;
        }
        if let Some(n) = next {
            self.entries[n].previous = previous;
        } else {
            self.last = previous;
        }
    }

    fn append(&mut self, i: usize) {
        self.entries[i].previous = self.last;
        self.entries[i].next = None;
        if let Some(last) = self.last {
            self.entries[last].next = Some(i);
        } else {
            self.first = Some(i);
        }
        self.last = Some(i);
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let &i = self.lookup.get(key)?;
        if self.last != Some(i) {
            self.unlink(i);
            self.append(i);
        }
        Some(&self.entries[i].value)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&i) = self.lookup.get(&key) {
            self.entries[i].value = value;
            self.unlink(i);
            self.append(i);
            return;
        }
        let i = if self.entries.len() == self.capacity {
            let i = self.first.expect("nonempty LRU has a first entry");
            self.unlink(i);
            self.lookup.remove(&self.entries[i].key);
            self.entries[i] = Entry {
                key: key.clone(),
                value,
                previous: None,
                next: None,
            };
            i
        } else {
            self.entries.push(Entry {
                key: key.clone(),
                value,
                previous: None,
                next: None,
            });
            self.entries.len() - 1
        };
        self.lookup.insert(key, i);
        self.append(i);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;

    #[test]
    fn refresh_and_replacement_evict_only_the_least_recent_entry() {
        let mut cache = Cache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        assert_eq!(cache.get(&"a"), Some(&1));
        cache.insert("c", 3);
        assert_eq!(cache.get(&"b"), None);
        cache.insert("a", 4);
        cache.insert("d", 5);
        assert_eq!(cache.get(&"c"), None);
        assert_eq!(cache.get(&"a"), Some(&4));
        assert_eq!(cache.get(&"d"), Some(&5));
        assert_eq!(cache.len(), 2);
    }
}
