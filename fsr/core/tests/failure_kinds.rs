//! One failure kind per HTTP error status.

use snapfire_fsr_core::ext::FailureKind;

#[test]
fn every_kind_reads_back_from_its_name_and_its_status() {
  let mut names = Vec::new();
  let mut statuses = Vec::new();
  for &kind in FailureKind::ALL {
    assert_eq!(FailureKind::from_name(kind.as_str()), Some(kind), "{kind:?}");
    assert_eq!(FailureKind::from_status(kind.http_status()), kind, "{kind:?}");
    names.push(kind.as_str());
    statuses.push(kind.http_status());
  }
  let mut sorted = statuses.clone();
  sorted.sort();
  sorted.dedup();
  assert_eq!(sorted, statuses, "one kind per status, in status order");
  names.sort();
  names.dedup();
  assert_eq!(names.len(), FailureKind::ALL.len(), "one name per kind");
}

#[test]
fn every_error_status_the_iana_registry_assigns_has_a_kind() {
  let defined = [
    400, 401, 402, 403, 404, 405, 406, 407, 408, 409, 410, 411, 412, 413, 414, 415, 416, 417, 421, 422, 423, 424, 425, 426, 428, 429, 431, 451, 500, 501, 502, 503, 504, 505, 506, 507, 508, 510, 511,
  ];
  let statuses: Vec<u16> = FailureKind::ALL.iter().map(|k| k.http_status()).collect();
  assert_eq!(statuses, defined);
}

#[test]
fn the_seven_first_kinds_keep_their_names_and_statuses() {
  for (kind, name, status) in [
    (FailureKind::Invalid, "invalid", 400),
    (FailureKind::Unauthorized, "unauthorized", 401),
    (FailureKind::NotFound, "not_found", 404),
    (FailureKind::Conflict, "conflict", 409),
    (FailureKind::Internal, "internal", 500),
    (FailureKind::Unavailable, "unavailable", 503),
    (FailureKind::Timeout, "timeout", 504),
  ] {
    assert_eq!((kind.as_str(), kind.http_status()), (name, status));
  }
}

#[test]
fn a_status_no_kind_names_falls_to_its_class() {
  assert_eq!(FailureKind::from_status(418), FailureKind::Invalid);
  assert_eq!(FailureKind::from_status(499), FailureKind::Invalid);
  assert_eq!(FailureKind::from_status(520), FailureKind::Internal);
  assert_eq!(FailureKind::from_status(302), FailureKind::Internal);
  assert_eq!(FailureKind::from_name("teapot"), None);
  assert!(FailureKind::Forbidden.is_client_error() && !FailureKind::Forbidden.is_server_error());
  assert!(FailureKind::BadGateway.is_server_error() && !FailureKind::BadGateway.is_client_error());
}
