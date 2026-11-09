use super::*;

/// The constant the instance member `name` of `class` runs: its own, else the nearest one up its chain.
pub(super) fn class_member(consts: Option<&Consts>, class: &str, name: &str, getter: bool) -> Option<String> {
  let consts = consts?;
  let mut at = class.to_owned();
  for _ in 0..64 {
    let key = if getter { class_getter_key(&at, name) } else { class_method_key(&at, name) };
    if consts.contains_key(&key) {
      return Some(key);
    }
    match consts.get(&class_parent_key(&at)) {
      Some(Expr::Lit(Lit::Str(parent))) => at = parent.clone(),
      _ => return None,
    }
  }
  None
}

/// The class `value` was made by, when it is an instance of one the build lowered.
/// Whether `a` and `b` are one object or one array.
pub(super) fn same_object(a: &Value, b: &Value) -> bool {
  match (a, b) {
    (Value::Map(a), Value::Map(b)) => a.same(b),
    (Value::Seq(a), Value::Seq(b)) => a.same(b),
    _ => false,
  }
}

/// Whether `value` is an object that decides its own primitive: a class instance or an object with its own `toString` or `valueOf`.
pub(super) fn converts_itself(value: &Value) -> bool {
  match value {
    Value::Map(map) => map.kind() == Collection::Plain && (map.contains_key(CLASS_FIELD) || map.contains_key("toString") || map.contains_key("valueOf")),
    _ => false,
  }
}

pub(super) fn instance_class(value: &Value) -> Option<String> {
  match value {
    Value::Map(fields) => match fields.get(CLASS_FIELD) {
      Some(Value::Str(class)) => Some(class.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// What a `catch` binds for `fail`: what was thrown; when nothing was, an `Error` with the failure's `kind` and `message`. `None` for a suspension, a deferred value thrown for a Suspense boundary to wait on, which no `catch` takes.
pub(super) fn caught(fail: &Fail) -> Option<Value> {
  match fail.thrown.as_deref() {
    Some(Value::Map(map)) if map.contains_key(crate::render::DEFERRED) => None,
    Some(thrown) => Some(thrown.clone()),
    None => {
      let mut fields = ValueMap::default();
      fields.insert("name".to_owned(), Value::str("Error"));
      fields.insert("kind".to_owned(), Value::str(fail.kind.as_str()));
      fields.insert("message".to_owned(), Value::str(fail.message.clone()));
      Some(Value::Map(fields))
    }
  }
}

/// The failure a `throw` is: `args` are the message and, when there is one, the value thrown. A thrown value whose `kind` names a failure kind fails with that kind, so a `catch` that throws what it caught fails as the original did.
#[inline(never)]
pub(super) fn thrown_fail(args: &[Value]) -> Fail {
  let thrown = args.get(1).or(args.first()).cloned().unwrap_or(Value::Undefined);
  let kind = match &thrown {
    Value::Map(fields) => fields.get("kind").and_then(|kind| match kind {
      Value::Str(kind) => FailureKind::from_name(kind.as_str()),
      _ => None,
    }),
    _ => None,
  };
  let message = args.first().map(|message| stringify(message).unwrap_or_else(|_| "an object thrown".to_owned())).unwrap_or_default();
  Fail::new(kind.unwrap_or(FailureKind::Internal), message).with_thrown(thrown)
}

/// How a `try` ends once its `finally` has run: `after` when the `finally` returned or failed, `before` when it ran to its end.
pub(super) fn after_finally(before: Result<Flow, Fail>, after: Result<Flow, Fail>) -> Result<Flow, Fail> {
  match after {
    Ok(Flow::Next) => before,
    other => other,
  }
}
