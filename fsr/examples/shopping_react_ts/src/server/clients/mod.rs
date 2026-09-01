//! Every service this application calls. One registry, one transport per
//! service, and one contract merged from what each service publishes.

pub mod shopping;

use std::sync::Arc;

use snapfire_fsr_service::{
  Contract, IdentityInterceptor, Services, TraceInterceptor,
};

fn merge(into: &mut Contract, from: Contract) {
  into.types.extend(from.types);
  into.services.extend(from.services);
}

pub fn build(shopping_url: &str) -> Arc<Services> {
  let shopping = shopping::import();

  let mut contract = Contract::new();
  merge(&mut contract, shopping.contract.clone());

  Services::builder()
    .contract(contract)
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .transport(shopping::NAME, Arc::new(shopping::transport(shopping_url, &shopping)))
    .build()
}
