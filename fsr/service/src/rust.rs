//! A service written in Rust: the `#[service]` attribute in
//! `snapfire_fsr_macros` writes a `Transport` for the marked `impl` and a
//! `DeclaredService` whose contract the host merges at boot. The types the
//! signatures name project onto the contract through `ContractType`, so the
//! compiler resolves each one rather than a table of names.

use std::collections::{BTreeMap, HashMap};

use indexmap::IndexMap;
use snapfire_fsr_runtime::ServiceError;

use crate::contract::{Contract, Type};

/// The contract type a Rust type crosses the boundary as. `define` adds the
/// records the type mentions to a contract; a scalar adds nothing.
pub trait ContractType {
  fn contract_type() -> Type;

  fn define(contract: &mut Contract) {
    let _ = contract;
  }
}

/// A type whose contract the `#[service]` attribute wrote. `NAME` is the
/// service name, the type's name in snake case.
pub trait DeclaredService {
  const NAME: &'static str;

  fn contract() -> Contract;
}

macro_rules! scalar {
  ($($t:ty => $ty:expr),* $(,)?) => {$(
    impl ContractType for $t {
      fn contract_type() -> Type {
        $ty
      }
    }
  )*};
}

scalar! {
  () => Type::Null,
  bool => Type::Bool,
  i8 => Type::I32,
  i16 => Type::I32,
  i32 => Type::I32,
  u8 => Type::U32,
  u16 => Type::U32,
  u32 => Type::U32,
  i64 => Type::I64,
  isize => Type::I64,
  u64 => Type::U64,
  usize => Type::U64,
  i128 => Type::I128,
  u128 => Type::U128,
  f32 => Type::F32,
  f64 => Type::F64,
  String => Type::Str,
  str => Type::Str,
}

impl<T: ContractType + ?Sized> ContractType for &T {
  fn contract_type() -> Type {
    T::contract_type()
  }

  fn define(contract: &mut Contract) {
    T::define(contract);
  }
}

impl<T: ContractType> ContractType for Option<T> {
  fn contract_type() -> Type {
    Type::optional(T::contract_type())
  }

  fn define(contract: &mut Contract) {
    T::define(contract);
  }
}

impl<T: ContractType> ContractType for Vec<T> {
  fn contract_type() -> Type {
    Type::list(T::contract_type())
  }

  fn define(contract: &mut Contract) {
    T::define(contract);
  }
}

macro_rules! string_map {
  ($($m:ident),*) => {$(
    impl<T: ContractType> ContractType for $m<String, T> {
      fn contract_type() -> Type {
        Type::map(T::contract_type())
      }

      fn define(contract: &mut Contract) {
        T::define(contract);
      }
    }
  )*};
}

string_map!(HashMap, BTreeMap, IndexMap);

/// A method that can fail answers `Result`; the contract sees the `Ok` type
/// and the error travels as the call's failure.
impl<T: ContractType> ContractType for Result<T, ServiceError> {
  fn contract_type() -> Type {
    T::contract_type()
  }

  fn define(contract: &mut Contract) {
    T::define(contract);
  }
}
