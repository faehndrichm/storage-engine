//! Integration tests for the public B+ tree API, run against the in-memory store and a
//! `BufferPool` that writes through to a file on disk.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

use storage_engine::{
    BPlusTree, TreeConfig,
    buffer_pool::{BufferPool, InMemoryPageStore, PageStore},
    disk::Disk,
};

/// Deterministic xorshift generator, so failures can be reproduced from the seed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % n as u64) as u32
    }

    fn shuffle(&mut self, values: &mut [u32]) {
        for i in (1..values.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            values.swap(i, j);
        }
    }
}

/// A page file in the temp dir which is removed again when the test ends.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "storage-engine-{}-{unique}-{name}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn store(&self) -> Arc<dyn PageStore> {
        let disk = Disk::create(self.0.to_str().unwrap()).unwrap();
        Arc::new(BufferPool::new(disk))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn in_memory() -> Arc<dyn PageStore> {
    Arc::new(InMemoryPageStore::new())
}

fn shuffled(range: std::ops::Range<u32>, seed: u64) -> Vec<u32> {
    let mut values: Vec<u32> = range.collect();
    Rng::new(seed).shuffle(&mut values);
    values
}

// scenarios, each one takes the store it should run against

fn insert_then_read_back(store: Arc<dyn PageStore>, order: usize, keys: Vec<u32>) {
    let mut tree = BPlusTree::new(store, TreeConfig::new(order));
    let mut model = BTreeSet::new();

    for &key in &keys {
        assert_eq!(tree.insert(key), model.insert(key), "insert {key}");
    }

    for &key in &keys {
        assert!(tree.find(key), "lost key {key}");
    }
    assert!(!tree.find(u32::MAX), "found a key that was never inserted");
    assert_eq!(
        tree.range(None, None),
        model.iter().copied().collect::<Vec<_>>()
    );
}

fn duplicate_inserts_are_rejected(store: Arc<dyn PageStore>) {
    let mut tree = BPlusTree::new(store, TreeConfig::new(4));

    for key in 0..100 {
        assert!(tree.insert(key));
    }
    for key in 0..100 {
        assert!(!tree.insert(key), "duplicate {key} was accepted");
    }
    assert_eq!(tree.range(None, None), (0..100).collect::<Vec<_>>());
}

fn range_bounds_match_model(store: Arc<dyn PageStore>, order: usize, seed: u64) {
    let mut tree = BPlusTree::new(store, TreeConfig::new(order));
    let mut model = BTreeSet::new();
    let mut rng = Rng::new(seed);

    for _ in 0..500 {
        let key = rng.below(1000);
        tree.insert(key);
        model.insert(key);
    }

    for _ in 0..200 {
        // bounds may be missing, present, or past both ends of the stored keys
        let from = rng.below(1100);
        let to = rng.below(1100);
        let expected: Vec<u32> = if from <= to {
            model.range(from..=to).copied().collect()
        } else {
            Vec::new()
        };
        if from <= to {
            assert_eq!(tree.range(Some(from), Some(to)), expected, "{from}..={to}");
        }
        assert_eq!(
            tree.range(Some(from), None),
            model.range(from..).copied().collect::<Vec<_>>(),
            "{from}.."
        );
        assert_eq!(
            tree.range(None, Some(to)),
            model.range(..=to).copied().collect::<Vec<_>>(),
            "..={to}"
        );
    }
}

fn delete_everything(store: Arc<dyn PageStore>, order: usize, seed: u64) {
    let mut tree = BPlusTree::new(store, TreeConfig::new(order));
    let keys = shuffled(0..1000, seed);

    for &key in &keys {
        assert!(tree.insert(key));
    }

    let mut remaining: BTreeSet<u32> = keys.iter().copied().collect();
    for key in shuffled(0..1000, seed + 1) {
        assert!(tree.delete(key), "delete {key}");
        remaining.remove(&key);
        assert!(!tree.find(key), "{key} still found after delete");
        assert!(!tree.delete(key), "{key} deleted twice");

        if key % 50 == 0 {
            assert_eq!(
                tree.range(None, None),
                remaining.iter().copied().collect::<Vec<_>>(),
                "range after deleting {key}"
            );
        }
    }

    assert!(tree.range(None, None).is_empty());
}

fn random_operations_match_model(store: Arc<dyn PageStore>, order: usize, seed: u64) {
    let mut tree = BPlusTree::new(store, TreeConfig::new(order));
    let mut model = BTreeSet::new();
    let mut rng = Rng::new(seed);

    for step in 0..5000 {
        // a small key range makes deletes hit existing keys most of the time
        let key = rng.below(200);

        if rng.below(2) == 0 {
            assert_eq!(
                tree.insert(key),
                model.insert(key),
                "seed {seed} step {step}: insert {key}"
            );
        } else {
            assert_eq!(
                tree.delete(key),
                model.remove(&key),
                "seed {seed} step {step}: delete {key}"
            );
        }

        assert_eq!(
            tree.find(key),
            model.contains(&key),
            "seed {seed} step {step}: find {key}"
        );

        if step % 100 == 0 {
            assert_eq!(
                tree.range(None, None),
                model.iter().copied().collect::<Vec<_>>(),
                "seed {seed} step {step}: full range"
            );
        }
    }

    assert_eq!(
        tree.range(None, None),
        model.iter().copied().collect::<Vec<_>>(),
        "seed {seed}: final full range"
    );
}

// in memory

#[test]
fn in_memory_insert_ascending() {
    for order in [4, 5, 6, 7] {
        insert_then_read_back(in_memory(), order, (0..2000).collect());
    }
}

#[test]
fn in_memory_insert_descending() {
    for order in [4, 5, 6, 7] {
        insert_then_read_back(in_memory(), order, (0..2000).rev().collect());
    }
}

#[test]
fn in_memory_insert_shuffled() {
    for order in [4, 5, 6, 7] {
        insert_then_read_back(in_memory(), order, shuffled(0..2000, 7));
    }
}

#[test]
fn in_memory_duplicate_inserts_are_rejected() {
    duplicate_inserts_are_rejected(in_memory());
}

#[test]
fn in_memory_range_bounds_match_model() {
    for order in [4, 5, 6, 7] {
        range_bounds_match_model(in_memory(), order, 11);
    }
}

#[test]
fn in_memory_delete_everything() {
    for order in [4, 5, 6, 7] {
        delete_everything(in_memory(), order, 3);
    }
}

#[test]
fn in_memory_random_operations_match_model() {
    for order in [4, 5, 6, 7] {
        for seed in 1..=5 {
            random_operations_match_model(in_memory(), order, seed);
        }
    }
}

// on disk

#[test]
fn on_disk_insert_ascending() {
    let file = TempFile::new("ascending");
    insert_then_read_back(file.store(), 4, (0..2000).collect());
}

#[test]
fn on_disk_insert_shuffled() {
    for order in [4, 6] {
        let file = TempFile::new("shuffled");
        insert_then_read_back(file.store(), order, shuffled(0..2000, 7));
    }
}

#[test]
fn on_disk_duplicate_inserts_are_rejected() {
    let file = TempFile::new("duplicates");
    duplicate_inserts_are_rejected(file.store());
}

#[test]
fn on_disk_range_bounds_match_model() {
    let file = TempFile::new("range");
    range_bounds_match_model(file.store(), 5, 11);
}

#[test]
fn on_disk_delete_everything() {
    let file = TempFile::new("delete-all");
    delete_everything(file.store(), 4, 3);
}

#[test]
fn on_disk_random_operations_match_model() {
    for seed in 1..=3 {
        let file = TempFile::new("random");
        random_operations_match_model(file.store(), 5, seed);
    }
}
