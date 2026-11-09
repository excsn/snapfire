#![allow(unused_imports)]

use std::time::Duration;

use snapfire_fsr_service::{TransportError, TransportOptions};

fn unsupported(error: &TransportError, option: &str) -> bool {
  matches!(error, TransportError::Unsupported { option: o, .. } if o == option)
}

const NOT_PEM: &[u8] = b"not a pem document";

#[cfg(feature = "http")]
mod http {
  use snapfire_fsr_service::HttpTransport;

  use super::*;

  #[test]
  fn a_root_certificate_that_is_not_der_is_a_root_certificate_error() {
    let error = HttpTransport::with_options("https://api.invalid", &TransportOptions::new().root_certificate(vec![1, 2, 3])).err().unwrap();
    assert!(matches!(error, TransportError::RootCertificate(_)), "{error}");
  }

  #[test]
  fn a_proxy_url_the_client_cannot_use_is_a_proxy_error() {
    let error = HttpTransport::with_options("http://api.invalid", &TransportOptions::new().proxy("ftp://proxy.invalid")).err().unwrap();
    assert!(matches!(error, TransportError::Proxy { .. }), "{error}");
  }

  #[test]
  fn a_client_identity_is_unsupported_over_http() {
    let error = HttpTransport::with_options("https://api.invalid", &TransportOptions::new().identity(NOT_PEM.to_vec(), NOT_PEM.to_vec())).err().unwrap();
    assert!(unsupported(&error, "identity"), "{error}");
  }

  #[test]
  fn keep_alive_and_max_connections_are_accepted_over_http() {
    let options = TransportOptions::new().max_connections(4).keep_alive(Duration::from_secs(10), Duration::from_secs(5));
    assert!(HttpTransport::with_options("http://api.invalid", &options).is_ok());
  }
}

#[cfg(feature = "graphql")]
mod graphql {
  use snapfire_fsr_service::{import_graphql, GraphqlTransport};

  use super::*;

  const SCHEMA: &str = "type Query { ping: String }\nquery Ping { ping }\n";

  fn imported() -> snapfire_fsr_service::ImportedGraphql {
    import_graphql(SCHEMA, "pinger").unwrap()
  }

  #[test]
  fn a_root_certificate_that_is_not_der_is_a_root_certificate_error() {
    let error = GraphqlTransport::with_options("https://api.invalid/graphql", &imported(), &TransportOptions::new().root_certificate(vec![1, 2, 3])).err().unwrap();
    assert!(matches!(error, TransportError::RootCertificate(_)), "{error}");
  }

  #[test]
  fn a_proxy_url_the_client_cannot_use_is_a_proxy_error() {
    let error = GraphqlTransport::with_options("http://api.invalid/graphql", &imported(), &TransportOptions::new().proxy("ftp://proxy.invalid")).err().unwrap();
    assert!(matches!(error, TransportError::Proxy { .. }), "{error}");
  }

  #[test]
  fn a_client_identity_is_unsupported_over_graphql() {
    let error = GraphqlTransport::with_options("https://api.invalid/graphql", &imported(), &TransportOptions::new().identity(NOT_PEM.to_vec(), NOT_PEM.to_vec())).err().unwrap();
    assert!(unsupported(&error, "identity"), "{error}");
  }

  #[test]
  fn keep_alive_and_max_connections_are_accepted_over_graphql() {
    let options = TransportOptions::new().max_connections(4).keep_alive(Duration::from_secs(10), Duration::from_secs(5));
    assert!(GraphqlTransport::with_options("http://api.invalid/graphql", &imported(), &options).is_ok());
  }
}

#[cfg(feature = "grpc")]
mod grpc {
  use snapfire_fsr_service::{import_proto_source, GrpcTransport, ImportedProto};

  use super::*;

  const PROTO: &str = "syntax = \"proto3\"; package p; message M { string s = 1; } service S { rpc Get (M) returns (M); }";

  fn imported() -> ImportedProto {
    import_proto_source("p.proto", PROTO, "p").unwrap()
  }

  #[test]
  fn a_root_certificate_that_is_not_der_is_a_root_certificate_error() {
    let error = GrpcTransport::with_options("https://api.invalid:443", &imported(), &TransportOptions::new().root_certificate(vec![1, 2, 3])).err().unwrap();
    assert!(matches!(error, TransportError::RootCertificate(_)), "{error}");
  }

  #[test]
  fn a_client_identity_that_is_not_pem_is_an_identity_error() {
    let error = GrpcTransport::with_options("https://api.invalid:443", &imported(), &TransportOptions::new().identity(NOT_PEM.to_vec(), NOT_PEM.to_vec())).err().unwrap();
    assert!(matches!(error, TransportError::Identity(_)), "{error}");
  }

  #[test]
  fn a_proxy_or_a_connection_limit_is_unsupported_over_grpc() {
    let proxy = GrpcTransport::with_options("http://api.invalid:50051", &imported(), &TransportOptions::new().proxy("http://proxy.invalid:3128")).err().unwrap();
    assert!(unsupported(&proxy, "proxy"), "{proxy}");
    let limit = GrpcTransport::with_options("http://api.invalid:50051", &imported(), &TransportOptions::new().max_connections(4)).err().unwrap();
    assert!(unsupported(&limit, "max_connections"), "{limit}");
  }

  #[test]
  fn a_base_url_that_does_not_parse_is_a_url_error() {
    let error = GrpcTransport::with_options("not a url", &imported(), &TransportOptions::new()).err().unwrap();
    assert!(matches!(error, TransportError::Url { .. }), "{error}");
  }

  #[test]
  fn keep_alive_is_accepted_over_grpc() {
    let options = TransportOptions::new().keep_alive(Duration::from_secs(10), Duration::from_secs(5));
    assert!(GrpcTransport::with_options("http://api.invalid:50051", &imported(), &options).is_ok());
  }
}
