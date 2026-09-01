use std::sync::Arc;

use futures_util::future::BoxFuture;
use indexmap::IndexMap;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, Identity, ServiceCaller, ServiceError, ServiceHandle};

use crate::call::{Call, Credentials, NoCredentials};
use crate::contract::Contract;
use crate::interceptor::{Chain, Interceptor, Next};
use crate::transport::Transport;

/// Named clients over pooled transports behind one seam. The contract is the
/// artifact from SERVICES.md section 2, read as data rather than generated in.
pub struct Services {
  contract: Arc<Contract>,
  chains: IndexMap<String, Arc<Chain>>,
  default_chain: Option<Arc<Chain>>,
  check_responses: bool,
}

#[derive(Default)]
pub struct ServicesBuilder {
  contract: Contract,
  interceptors: Vec<Arc<dyn Interceptor>>,
  transports: IndexMap<String, Arc<dyn Transport>>,
  default_transport: Option<Arc<dyn Transport>>,
  check_responses: bool,
}

impl Services {
  pub fn builder() -> ServicesBuilder {
    ServicesBuilder { check_responses: true, ..Default::default() }
  }

  pub fn contract(&self) -> &Contract {
    &self.contract
  }

  /// Binds the layer to one request. The edge calls this with the session's
  /// identity and token custody; what comes back is safe to hand to
  /// application code because it only calls.
  pub fn bind(
    self: &Arc<Self>,
    identity: Option<Identity>,
    credentials: Arc<dyn Credentials>,
  ) -> ServiceHandle {
    ServiceHandle::new(Arc::new(BoundServices {
      services: self.clone(),
      identity,
      credentials,
    }))
  }

  pub fn bind_anonymous(self: &Arc<Self>) -> ServiceHandle {
    self.bind(None, Arc::new(NoCredentials))
  }

  fn chain_for(&self, service: &str) -> Option<&Arc<Chain>> {
    self.chains.get(service).or(self.default_chain.as_ref())
  }
}

impl ServicesBuilder {
  pub fn contract(mut self, contract: Contract) -> Self {
    self.contract = contract;
    self
  }

  pub fn intercept(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
    self.interceptors.push(interceptor);
    self
  }

  pub fn transport(mut self, service: impl Into<String>, transport: Arc<dyn Transport>) -> Self {
    self.transports.insert(service.into(), transport);
    self
  }

  pub fn default_transport(mut self, transport: Arc<dyn Transport>) -> Self {
    self.default_transport = Some(transport);
    self
  }

  /// Off only when a backend is trusted to honour its own contract and the
  /// check is measurably in the way.
  pub fn check_responses(mut self, check: bool) -> Self {
    self.check_responses = check;
    self
  }

  pub fn build(self) -> Arc<Services> {
    let chain_over = |transport: Arc<dyn Transport>| {
      Arc::new(Chain { interceptors: self.interceptors.clone(), transport })
    };
    Arc::new(Services {
      contract: Arc::new(self.contract),
      chains: self.transports.into_iter().map(|(name, t)| (name, chain_over(t))).collect(),
      default_chain: self.default_transport.map(chain_over),
      check_responses: self.check_responses,
    })
  }
}

struct BoundServices {
  services: Arc<Services>,
  identity: Option<Identity>,
  credentials: Arc<dyn Credentials>,
}

impl ServiceCaller for BoundServices {
  fn call(
    &self,
    service: &str,
    method: &str,
    args: ValueMap,
  ) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let contract = self.services.contract.clone();
    if let Err(e) = contract.check_call(service, method, &args) {
      let kind = match e {
        crate::check::ContractError::UnknownService(_)
        | crate::check::ContractError::UnknownMethod { .. } => FailureKind::NotFound,
        _ => FailureKind::Invalid,
      };
      let error = ServiceError::new(kind, service, method, e.to_string());
      return Box::pin(async move { Err(error) });
    }

    let Some(chain) = self.services.chain_for(service) else {
      let error = ServiceError::new(
        FailureKind::Unavailable,
        service,
        method,
        format!("no transport bound for `{service}`"),
      );
      return Box::pin(async move { Err(error) });
    };

    let call = Call {
      service: service.to_owned(),
      method: method.to_owned(),
      args,
      identity: self.identity.clone(),
      metadata: ValueMap::new(),
      credentials: self.credentials.clone(),
    };
    let running = Next::start(chain.clone()).run(call);
    let check = self.services.check_responses;
    let service = service.to_owned();
    let method = method.to_owned();
    Box::pin(async move {
      let value = running.await?;
      if check {
        contract.check_return(&service, &method, &value).map_err(|e| {
          ServiceError::new(FailureKind::Internal, &service, &method, e.to_string())
        })?;
      }
      Ok(value)
    })
  }
}
