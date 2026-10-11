//! A bounded map whose entries expire a fixed time after insertion.

use std::{
  borrow::Borrow,
  collections::{HashMap, VecDeque},
  hash::Hash,
  sync::Arc,
  time::{Duration, Instant},
};

use parking_lot::Mutex;

/// Cheap to clone; clones share one map.
#[derive(Clone)]
pub struct TtlCache<K, V> {
  ttl:      Duration,
  capacity: usize,
  inner:    Arc<Mutex<Inner<K, V>>>,
}

struct Inner<K, V> {
  entries: HashMap<K, (Instant, V)>,
  /// Insertion order, oldest first. A key inserted again leaves its older
  /// position behind, which is skipped when it reaches the front.
  order:   VecDeque<(Instant, K)>,
}

impl<K: Hash + Eq + Clone, V: Clone> TtlCache<K, V> {
  #[must_use]
  pub fn new(ttl: Duration, capacity: usize) -> Self {
    Self {
      ttl,
      capacity,
      inner: Arc::new(Mutex::new(Inner {
        entries: HashMap::new(),
        order:   VecDeque::new(),
      })),
    }
  }

  pub fn get<Q>(&self, key: &Q) -> Option<V>
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    let mut inner = self.inner.lock();
    let (inserted, value) = inner.entries.get(key)?;

    if inserted.elapsed() < self.ttl {
      return Some(value.clone());
    }

    inner.entries.remove(key);
    None
  }

  pub fn insert(&self, key: K, value: V) {
    let now = Instant::now();
    let mut inner = self.inner.lock();
    inner.entries.insert(key.clone(), (now, value));
    inner.order.push_back((now, key));

    while let Some((inserted, _)) = inner.order.front() {
      let over = inner.entries.len() > self.capacity
        || inner.order.len() > self.capacity.saturating_mul(2);
      if !over && inserted.elapsed() < self.ttl {
        break;
      }

      let Some((inserted, key)) = inner.order.pop_front() else {
        break;
      };
      if inner
        .entries
        .get(&key)
        .is_some_and(|(at, _)| *at == inserted)
      {
        inner.entries.remove(&key);
      }
    }

    drop(inner);
  }
}
