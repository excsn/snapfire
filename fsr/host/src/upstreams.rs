//! The addresses a site over a transport is reached at: taken in turn, an
//! address that failed rested for `retry` and tried again after.

use std::time::{Duration, Instant};

use parking_lot::Mutex;

pub struct Upstreams {
  addresses: Vec<String>,
  retry: Duration,
  state: Mutex<State>,
}

struct State {
  next: usize,
  /// When each address last failed; `None` for one that is up.
  down: Vec<Option<Instant>>,
}

/// One address to call, handed back as [`Upstreams::failed`] or
/// [`Upstreams::recovered`] with what the call found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
  pub index: usize,
  pub address: String,
}

impl Upstreams {
  pub fn new(addresses: Vec<String>, retry: Duration) -> Self {
    let down = vec![None; addresses.len()];
    Self { addresses, retry, state: Mutex::new(State { next: 0, down }) }
  }

  pub fn addresses(&self) -> &[String] {
    &self.addresses
  }

  /// The next address in turn among those up or rested. With every address
  /// down and none rested, the one down longest, so a site whose hosts all
  /// went away is probed rather than written off. `None` only for an empty
  /// list.
  pub fn pick(&self) -> Option<Lease> {
    if self.addresses.is_empty() {
      return None;
    }
    let now = Instant::now();
    let mut state = self.state.lock();
    let count = self.addresses.len();
    for step in 0..count {
      let index = (state.next + step) % count;
      let rested = match state.down[index] {
        None => true,
        Some(since) => now.duration_since(since) >= self.retry,
      };
      if rested {
        state.next = (index + 1) % count;
        return Some(self.lease(index));
      }
    }
    let index = (0..count).min_by_key(|i| state.down[*i]).expect("a non-empty list");
    state.next = (index + 1) % count;
    Some(self.lease(index))
  }

  /// The address answered: it is up again if it was down.
  pub fn recovered(&self, lease: &Lease) {
    self.state.lock().down[lease.index] = None;
  }

  /// The address did not answer: it rests for `retry` before its next turn.
  pub fn failed(&self, lease: &Lease) {
    self.state.lock().down[lease.index] = Some(Instant::now());
  }

  /// Which addresses are down now, by index.
  pub fn down(&self) -> Vec<usize> {
    let now = Instant::now();
    let state = self.state.lock();
    (0..self.addresses.len())
      .filter(|i| state.down[*i].is_some_and(|since| now.duration_since(since) < self.retry))
      .collect()
  }

  fn lease(&self, index: usize) -> Lease {
    Lease { index, address: self.addresses[index].clone() }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn three(retry: Duration) -> Upstreams {
    Upstreams::new(vec!["http://a".to_owned(), "http://b".to_owned(), "http://c".to_owned()], retry)
  }

  #[test]
  fn addresses_are_taken_in_turn() {
    let up = three(Duration::from_secs(5));
    let picked: Vec<String> = (0..4).map(|_| up.pick().unwrap().address).collect();
    assert_eq!(picked, ["http://a", "http://b", "http://c", "http://a"]);
  }

  #[test]
  fn a_failed_address_is_skipped_until_it_rested() {
    let up = three(Duration::from_millis(20));
    let a = up.pick().unwrap();
    up.failed(&a);
    let picked: Vec<String> = (0..3).map(|_| up.pick().unwrap().address).collect();
    assert_eq!(picked, ["http://b", "http://c", "http://b"]);
    assert_eq!(up.down(), [0]);
    std::thread::sleep(Duration::from_millis(25));
    assert!(up.down().is_empty());
    let picked: Vec<String> = (0..3).map(|_| up.pick().unwrap().address).collect();
    assert!(picked.contains(&"http://a".to_owned()), "{picked:?}");
  }

  #[test]
  fn with_every_address_down_the_one_down_longest_is_probed() {
    let up = three(Duration::from_secs(60));
    let a = up.pick().unwrap();
    up.failed(&a);
    let b = up.pick().unwrap();
    up.failed(&b);
    let c = up.pick().unwrap();
    up.failed(&c);
    assert_eq!(up.pick().unwrap().address, "http://a");
    up.recovered(&a);
    assert_eq!(up.down(), [1, 2]);
    assert_eq!(up.pick().unwrap().address, "http://a");
  }

  #[test]
  fn an_empty_list_leases_nothing() {
    assert_eq!(Upstreams::new(Vec::new(), Duration::from_secs(1)).pick(), None);
  }
}
