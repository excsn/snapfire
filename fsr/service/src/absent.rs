//! The gRPC and GraphQL names a caller reaches for, when the crate is built without the feature behind them: each import refuses, naming the feature to turn on, so a caller compiles the same either way.

#[cfg(not(feature = "grpc"))]
pub use grpc::{import_proto, GrpcTransport, ImportedProto};
#[cfg(not(feature = "graphql"))]
pub use graphql::{import_graphql, GraphqlTransport, ImportedGraphql};

#[cfg(not(feature = "grpc"))]
mod grpc {
  use std::convert::Infallible;
  use std::path::Path;

  use futures_util::future::BoxFuture;
  use snapfire_fsr_core::Value;

  use crate::{Call, Contract, ImportError, Transport};
  use snapfire_fsr_runtime::ServiceError;

  /// What a `.proto` document imports to. None is ever made without the feature.
  #[non_exhaustive]
  pub struct ImportedProto {
    pub contract: Contract,
    never: Infallible,
  }

  pub fn import_proto(path: &Path, _default_service: &str) -> Result<ImportedProto, ImportError> {
    Err(ImportError::Feature { at: path.display().to_string(), contract: "gRPC", feature: "grpc" })
  }

  pub struct GrpcTransport {
    never: Infallible,
  }

  impl GrpcTransport {
    pub fn new(_base: &str, imported: &ImportedProto) -> Result<Self, String> {
      match imported.never {}
    }

    pub fn with_options(_base: &str, imported: &ImportedProto, _options: &crate::TransportOptions) -> Result<Self, crate::TransportError> {
      match imported.never {}
    }

    pub fn metadata(self, _name: impl Into<String>, _value: impl Into<String>) -> Self {
      self
    }
  }

  impl Transport for GrpcTransport {
    fn call(&self, _call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
      match self.never {}
    }
  }
}

#[cfg(not(feature = "graphql"))]
mod graphql {
  use std::convert::Infallible;

  use futures_util::future::BoxFuture;
  use snapfire_fsr_core::Value;

  use crate::{Call, Contract, ImportError, Transport};
  use snapfire_fsr_runtime::ServiceError;

  /// What a `.graphql` document imports to. None is ever made without the feature.
  #[non_exhaustive]
  pub struct ImportedGraphql {
    pub contract: Contract,
    never: Infallible,
  }

  pub fn import_graphql(_source: &str, service: &str) -> Result<ImportedGraphql, ImportError> {
    Err(ImportError::Feature { at: format!("{service}.graphql"), contract: "GraphQL", feature: "graphql" })
  }

  pub struct GraphqlTransport {
    never: Infallible,
  }

  impl GraphqlTransport {
    pub fn new(_endpoint: impl Into<String>, imported: &ImportedGraphql) -> Self {
      match imported.never {}
    }

    pub fn with_options(_endpoint: impl Into<String>, imported: &ImportedGraphql, _options: &crate::TransportOptions) -> Result<Self, crate::TransportError> {
      match imported.never {}
    }

    pub fn header(self, _name: impl Into<String>, _value: impl Into<String>) -> Self {
      self
    }

    pub fn query(self, _name: impl Into<String>, _value: impl Into<String>) -> Self {
      self
    }
  }

  impl Transport for GraphqlTransport {
    fn call(&self, _call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
      match self.never {}
    }
  }
}
