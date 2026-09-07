use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};

use crate::actions::FailureKind;
use crate::services::ServiceError;

/// One module of the application's own Rust, reached from a body as
/// `ctx.native.<name>.<method>()`. Nothing crosses a wire, so there is no
/// contract to check the call against, no transport to select and no
/// interceptor chain: the arguments are the ones the build read off the Rust
/// signature and the answer is whatever that function returned.
///
/// A module composes with another as ordinary Rust, by holding it, which is
/// why only the methods the build found on the marked `impl` are reachable
/// from a body and everything else stays private.
pub trait Native: Send + Sync {
  fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>;

  /// A method the build read as `fn` rather than `async fn`, answered without
  /// a future so it can run where nothing may suspend. `None` means the name
  /// is async or unknown, and the caller must use `call`.
  fn call_sync(&self, method: &str, args: ValueMap) -> Option<Result<Value, ServiceError>> {
    let _ = (method, args);
    None
  }
}

/// `ctx.native`. Empty unless the host registered something, and an unbound
/// handle fails the call rather than pretending.
#[derive(Clone, Default)]
pub struct NativeHandle(Option<Arc<Natives>>);

/// The registered modules, by the name a body calls them under.
#[derive(Default)]
pub struct Natives {
  modules: Vec<(String, Arc<dyn Native>)>,
}

impl Natives {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(&mut self, name: impl Into<String>, module: Arc<dyn Native>) {
    let name = name.into();
    self.modules.retain(|(held, _)| *held != name);
    self.modules.push((name, module));
  }

  pub fn get(&self, name: &str) -> Option<&Arc<dyn Native>> {
    self.modules.iter().find(|(held, _)| held == name).map(|(_, module)| module)
  }

  pub fn is_empty(&self) -> bool {
    self.modules.is_empty()
  }

  /// Every registered name, in registration order, for the boot report.
  pub fn names(&self) -> Vec<&str> {
    self.modules.iter().map(|(name, _)| name.as_str()).collect()
  }
}

impl NativeHandle {
  pub fn new(natives: Arc<Natives>) -> Self {
    Self(Some(natives))
  }

  pub fn is_bound(&self) -> bool {
    self.0.is_some()
  }

  /// The sync half: `None` when nothing is registered under `module` or the
  /// method is not one the build read as `fn`.
  pub fn call_sync(&self, module: &str, method: &str, args: ValueMap) -> Option<Result<Value, ServiceError>> {
    self.0.as_ref()?.get(module)?.call_sync(method, args)
  }

  pub fn call(&self, module: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let found = self.0.as_ref().and_then(|natives| natives.get(module).cloned());
    match found {
      Some(native) => native.call(method, args),
      None => {
        let message = match self.0.is_some() {
          true => format!("no native module `{module}` is registered"),
          false => "no native modules are bound to this request".to_owned(),
        };
        let error = ServiceError::new(FailureKind::Unavailable, module, method, message);
        Box::pin(async move { Err(error) })
      }
    }
  }
}

/// One argument, decoded from the map a body passed. An absent or wrong-typed
/// argument is the caller's error rather than a panic, since the body is
/// lowered TypeScript and the Rust signature is what the build typed it
/// against.
pub fn native_arg<T: FromNativeValue>(args: &ValueMap, key: &str, method: &str) -> Result<T, ServiceError> {
  match args.get(key) {
    Some(value) => T::from_native_value(value.clone())
      .ok_or_else(|| ServiceError::new(FailureKind::Invalid, "native", method, format!("`{key}` is not the type `{method}` declares"))),
    None => T::from_native_value(Value::Null)
      .ok_or_else(|| ServiceError::new(FailureKind::Invalid, "native", method, format!("`{method}` needs `{key}`"))),
  }
}

/// What a native method's argument may be. Implemented for the value model's
/// scalars, `Option`, `Vec` and `Value` itself; a type outside it is a
/// compile error at the call the macro writes rather than a runtime surprise.
pub trait FromNativeValue: Sized {
  fn from_native_value(value: Value) -> Option<Self>;
}

/// What a native method may answer with.
pub trait IntoNativeValue {
  fn into_native_value(self) -> Value;
}

impl FromNativeValue for Value {
  fn from_native_value(value: Value) -> Option<Self> {
    Some(value)
  }
}

impl FromNativeValue for ValueMap {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::Map(map) => Some(map),
      _ => None,
    }
  }
}

impl FromNativeValue for String {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::Str(s) => Some(s),
      _ => None,
    }
  }
}

impl FromNativeValue for bool {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::Bool(b) => Some(b),
      _ => None,
    }
  }
}

macro_rules! from_int {
  ($($t:ty),*) => {$(
    impl FromNativeValue for $t {
      fn from_native_value(value: Value) -> Option<Self> {
        match value {
          Value::Int(n) => <$t>::try_from(n).ok(),
          Value::UInt(n) => <$t>::try_from(n).ok(),
          Value::F64(f) if f.fract() == 0.0 => Some(f as $t),
          _ => None,
        }
      }
    }
  )*};
}
from_int!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, usize, isize);

impl FromNativeValue for f64 {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::F64(f) => Some(f),
      Value::F32(f) => Some(f as f64),
      Value::Int(n) => Some(n as f64),
      _ => None,
    }
  }
}

impl<T: FromNativeValue> FromNativeValue for Option<T> {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::Null => Some(None),
      other => T::from_native_value(other).map(Some),
    }
  }
}

impl<T: FromNativeValue> FromNativeValue for Vec<T> {
  fn from_native_value(value: Value) -> Option<Self> {
    match value {
      Value::Seq(items) => items.into_iter().map(T::from_native_value).collect(),
      _ => None,
    }
  }
}

impl IntoNativeValue for Value {
  fn into_native_value(self) -> Value {
    self
  }
}

impl IntoNativeValue for ValueMap {
  fn into_native_value(self) -> Value {
    Value::Map(self)
  }
}

impl IntoNativeValue for String {
  fn into_native_value(self) -> Value {
    Value::Str(self)
  }
}

impl IntoNativeValue for bool {
  fn into_native_value(self) -> Value {
    Value::Bool(self)
  }
}

macro_rules! into_signed {
  ($($t:ty),*) => {$(
    impl IntoNativeValue for $t {
      fn into_native_value(self) -> Value {
        Value::Int(self as i128)
      }
    }
  )*};
}
into_signed!(i8, i16, i32, i64, i128, isize);

macro_rules! into_unsigned {
  ($($t:ty),*) => {$(
    impl IntoNativeValue for $t {
      fn into_native_value(self) -> Value {
        match i128::try_from(self) {
          Ok(n) => Value::Int(n),
          Err(_) => Value::UInt(self as u128),
        }
      }
    }
  )*};
}
into_unsigned!(u8, u16, u32, u64, u128, usize);

impl IntoNativeValue for f64 {
  fn into_native_value(self) -> Value {
    Value::F64(self)
  }
}

impl IntoNativeValue for f32 {
  fn into_native_value(self) -> Value {
    Value::F32(self)
  }
}

impl<T: IntoNativeValue> IntoNativeValue for Option<T> {
  fn into_native_value(self) -> Value {
    match self {
      Some(v) => v.into_native_value(),
      None => Value::Null,
    }
  }
}

impl<T: IntoNativeValue> IntoNativeValue for Vec<T> {
  fn into_native_value(self) -> Value {
    Value::Seq(self.into_iter().map(IntoNativeValue::into_native_value).collect())
  }
}

/// A method that can fail answers with `Result`, and the failure reaches the
/// body as the error the service layer already speaks.
impl<T: IntoNativeValue> IntoNativeValue for Result<T, ServiceError> {
  fn into_native_value(self) -> Value {
    match self {
      Ok(v) => v.into_native_value(),
      Err(e) => Value::Str(e.message),
    }
  }
}
