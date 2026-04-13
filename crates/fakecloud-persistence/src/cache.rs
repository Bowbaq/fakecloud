use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BodyKey {
    pub bucket: String,
    pub key: String,
    pub version: Option<String>,
}

impl BodyKey {
    pub fn new(bucket: impl Into<String>, key: impl Into<String>, version: Option<String>) -> Self {
        Self {
            bucket: bucket.into(),
            key: key.into(),
            version,
        }
    }
}

/// Per-entry accounting overhead charged to each cache entry in addition to
/// the raw body size. Without this, zero-length bodies accrue zero bytes
/// against the capacity and a pathological flood of empty objects would grow
/// the index unboundedly.
const PER_ENTRY_OVERHEAD: u64 = 128;

#[derive(Debug)]
struct Node {
    key: BodyKey,
    bytes: Bytes,
    charged: u64,
    prev: Option<usize>,
    next: Option<usize>,
}

struct Inner {
    capacity_bytes: u64,
    single_object_cap: u64,
    used_bytes: u64,
    nodes: Vec<Option<Node>>,
    free: Vec<usize>,
    index: HashMap<BodyKey, usize>,
    head: Option<usize>, // MRU
    tail: Option<usize>, // LRU
}

impl Inner {
    fn new(capacity_bytes: u64) -> Self {
        Self {
            capacity_bytes,
            single_object_cap: capacity_bytes / 2,
            used_bytes: 0,
            nodes: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            head: None,
            tail: None,
        }
    }

    fn alloc_node(&mut self, node: Node) -> usize {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = Some(node);
            idx
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        }
    }

    fn detach(&mut self, idx: usize) {
        let (prev, next) = {
            let n = self.nodes[idx].as_ref().unwrap();
            (n.prev, n.next)
        };
        match prev {
            Some(p) => self.nodes[p].as_mut().unwrap().next = next,
            None => self.head = next,
        }
        match next {
            Some(n) => self.nodes[n].as_mut().unwrap().prev = prev,
            None => self.tail = prev,
        }
        let n = self.nodes[idx].as_mut().unwrap();
        n.prev = None;
        n.next = None;
    }

    fn push_front(&mut self, idx: usize) {
        let old_head = self.head;
        {
            let n = self.nodes[idx].as_mut().unwrap();
            n.prev = None;
            n.next = old_head;
        }
        if let Some(h) = old_head {
            self.nodes[h].as_mut().unwrap().prev = Some(idx);
        }
        self.head = Some(idx);
        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    fn remove(&mut self, idx: usize) -> Node {
        self.detach(idx);
        let node = self.nodes[idx].take().unwrap();
        self.free.push(idx);
        self.used_bytes = self.used_bytes.saturating_sub(node.charged);
        self.index.remove(&node.key);
        node
    }

    fn evict_until_fits(&mut self, incoming: u64) {
        while self.used_bytes + incoming > self.capacity_bytes {
            let Some(tail) = self.tail else {
                break;
            };
            self.remove(tail);
        }
    }

    fn get(&mut self, key: &BodyKey) -> Option<Bytes> {
        let idx = *self.index.get(key)?;
        self.detach(idx);
        self.push_front(idx);
        Some(self.nodes[idx].as_ref().unwrap().bytes.clone())
    }

    fn insert(&mut self, key: BodyKey, bytes: Bytes) {
        // Always drop any existing entry for this key, even if the new body
        // is oversized and will bypass the cache — otherwise a stale older
        // body could be served after an overwrite.
        if let Some(&idx) = self.index.get(&key) {
            self.remove(idx);
        }
        let size = bytes.len() as u64;
        if size > self.single_object_cap {
            return;
        }
        let charged = size.saturating_add(PER_ENTRY_OVERHEAD);
        self.evict_until_fits(charged);
        self.used_bytes += charged;
        let node = Node {
            key: key.clone(),
            bytes,
            charged,
            prev: None,
            next: None,
        };
        let idx = self.alloc_node(node);
        self.index.insert(key, idx);
        self.push_front(idx);
    }

    fn invalidate(&mut self, key: &BodyKey) {
        if let Some(&idx) = self.index.get(key) {
            self.remove(idx);
        }
    }
}

#[derive(Clone)]
pub struct BodyCache {
    inner: Arc<Mutex<Inner>>,
    capacity_bytes: u64,
}

impl BodyCache {
    pub fn new(capacity_bytes: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new(capacity_bytes))),
            capacity_bytes,
        }
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    pub fn single_object_cap(&self) -> u64 {
        self.capacity_bytes / 2
    }

    pub fn get(&self, key: &BodyKey) -> Option<Bytes> {
        self.inner.lock().get(key)
    }

    pub fn insert(&self, key: BodyKey, bytes: Bytes) {
        self.inner.lock().insert(key, bytes);
    }

    pub fn invalidate(&self, key: &BodyKey) {
        self.inner.lock().invalidate(key);
    }

    #[cfg(test)]
    fn used_bytes(&self) -> u64 {
        self.inner.lock().used_bytes
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().index.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(name: &str) -> BodyKey {
        BodyKey::new("b", name, None)
    }

    fn mk(n: usize) -> Bytes {
        Bytes::from(vec![0u8; n])
    }

    #[test]
    fn insert_and_get() {
        let c = BodyCache::new(1024);
        c.insert(k("a"), mk(100));
        assert_eq!(c.get(&k("a")).unwrap().len(), 100);
        assert_eq!(c.used_bytes(), 100 + PER_ENTRY_OVERHEAD);
    }

    #[test]
    fn byte_accounting_on_overwrite() {
        let c = BodyCache::new(1024);
        c.insert(k("a"), mk(100));
        c.insert(k("a"), mk(50));
        assert_eq!(c.used_bytes(), 50 + PER_ENTRY_OVERHEAD);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn lru_eviction_on_capacity_pressure() {
        // Capacity 3 * (100 + overhead) so exactly three entries fit.
        let c = BodyCache::new(3 * (100 + PER_ENTRY_OVERHEAD));
        c.insert(k("a"), mk(100));
        c.insert(k("b"), mk(100));
        c.insert(k("c"), mk(100));
        // Access a to make it MRU; b is now LRU.
        let _ = c.get(&k("a"));
        c.insert(k("d"), mk(100));
        assert!(c.get(&k("b")).is_none());
        assert!(c.get(&k("a")).is_some());
        assert!(c.get(&k("c")).is_some());
        assert!(c.get(&k("d")).is_some());
    }

    #[test]
    fn empty_bodies_still_evict_under_entry_overhead() {
        // 1 MiB capacity: with 128 bytes of overhead per entry, the cache can
        // hold at most ~4096 zero-length entries before eviction kicks in.
        // Insert 10_000 distinct empty keys and assert the index is bounded.
        let c = BodyCache::new(1024 * 1024);
        for i in 0..10_000 {
            c.insert(k(&format!("empty-{i}")), Bytes::new());
        }
        let max_entries = (1024 * 1024 / PER_ENTRY_OVERHEAD) as usize;
        assert!(
            c.len() <= max_entries,
            "cache must evict empty bodies: len={} max={}",
            c.len(),
            max_entries
        );
        assert!(c.used_bytes() <= 1024 * 1024);
    }

    #[test]
    fn single_object_cap_bypass_leaves_cache_untouched() {
        // 64 MiB capacity, 32 MiB single-object cap
        let cap = 64 * 1024 * 1024;
        let c = BodyCache::new(cap);
        c.insert(k("a"), mk(1024));
        let before_used = c.used_bytes();
        let before_len = c.len();
        c.insert(k("big"), mk(40 * 1024 * 1024));
        assert_eq!(c.used_bytes(), before_used);
        assert_eq!(c.len(), before_len);
        assert!(c.get(&k("big")).is_none());
        assert!(c.get(&k("a")).is_some());
    }

    #[test]
    fn get_promotes_to_mru() {
        let c = BodyCache::new(3 * (100 + PER_ENTRY_OVERHEAD));
        c.insert(k("a"), mk(100));
        c.insert(k("b"), mk(100));
        c.insert(k("c"), mk(100));
        let _ = c.get(&k("a"));
        c.insert(k("d"), mk(100));
        // b should be evicted, a should remain.
        assert!(c.get(&k("a")).is_some());
        assert!(c.get(&k("b")).is_none());
    }

    #[test]
    fn oversized_overwrite_invalidates_previous_entry() {
        // 64 MiB capacity, 32 MiB single-object cap
        let cap = 64 * 1024 * 1024;
        let c = BodyCache::new(cap);
        c.insert(k("a"), mk(1024));
        assert!(c.get(&k("a")).is_some());
        // Overwrite with an oversized body: the new body bypasses, and the
        // previous entry must be invalidated so stale bytes are not served.
        c.insert(k("a"), mk(40 * 1024 * 1024));
        assert!(c.get(&k("a")).is_none());
        assert_eq!(c.used_bytes(), 0);
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn invalidate_removes_entry() {
        let c = BodyCache::new(1024);
        c.insert(k("a"), mk(100));
        c.invalidate(&k("a"));
        assert!(c.get(&k("a")).is_none());
        assert_eq!(c.used_bytes(), 0);
    }
}
