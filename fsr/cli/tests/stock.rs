//! `fsr serve`, the stock host in development, carries every feature of the host's `stock` list, as `fsr-server` does in production.
#![cfg(feature = "stock")]

#[test]
fn the_development_host_holds_every_stock_feature() {
  let missing: Vec<&str> = snapfire_fsr_host::features::STOCK.iter().copied().filter(|feature| !snapfire_fsr_host::features::built(feature)).collect();
  assert!(missing.is_empty(), "{missing:?}");
}
