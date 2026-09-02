pub mod call;
pub mod check;
pub mod contract;
pub mod http;
pub mod interceptor;
pub mod openapi;
#[cfg(feature = "grpc")]
pub mod proto;
#[cfg(feature = "grpc")]
pub mod grpc;
pub mod registry;
pub mod transport;
pub mod typescript;

pub use call::{Call, Credentials, NoCredentials};
pub use check::ContractError;
pub use contract::{Contract, Field, Method, ScalarKind, Service, Type, TypeDef, Variant};
pub use http::{kind_for_status, HttpTransport, Route};
pub use openapi::{import, ImportError, Imported};
#[cfg(feature = "grpc")]
pub use proto::{import_proto, import_proto_source, GrpcMethod, ImportedProto};
#[cfg(feature = "grpc")]
pub use grpc::GrpcTransport;
pub use interceptor::{
  CredentialInterceptor, IdentityInterceptor, Interceptor, Next, TraceInterceptor,
};
pub use registry::{Services, ServicesBuilder};
pub use transport::{LocalTransport, MockTransport, Transport};
