use super::*;

/// Function values are numbered in a table a parallel copy does not hold.
pub(super) fn makes_function_value(expr: &Expr) -> bool {
  match expr {
    Expr::Lambda { .. } | Expr::Fix { .. } => true,
    Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
      Entry::Field(_, value) | Entry::Computed(_, value) | Entry::Item(value) | Entry::Spread(value) => makes_function_value(value),
      Entry::Baked(_) => false,
    }),
    _ => false,
  }
}

/// How many statements from the front are `let`s that read none of each
/// other's names and make no function value. Two or more are evaluated together.
/// Whether a `let` can run beside others in a body that writes objects: nothing in it can reach a write.
pub(super) fn writes_nothing(expr: &Expr) -> bool {
  let mut safe = true;
  expr.visit(&mut |e| safe &= !matches!(e, Expr::Apply { .. } | Expr::ApplyOf(..) | Expr::Builtin { name: Builtin::Overwrite | Builtin::HeapOn, .. } | Expr::Lambda { .. }));
  safe
}

/// `value` with every object and array in it a new one of its own.
pub(super) fn own_copy(value: &Value) -> Value {
  match value {
    Value::Map(map) => {
      let mut out = snapfire_fsr_core::Fields::with_capacity(map.len());
      for (k, v) in map.iter() {
        out.insert(k.clone(), own_copy(v));
      }
      Value::Map(out.into())
    }
    Value::Seq(items) => Value::seq(items.iter().map(own_copy).collect::<Vec<_>>()),
    other => other.clone(),
  }
}

/// `Object.values(value)`.
pub(crate) fn values_of(value: Value) -> Result<Value, Fail> {
  match value {
    Value::Map(map) => Ok(Value::Seq(map.into_fields().into_values().collect())),
    other => Err(type_error("Object.values", "an object", &other)),
  }
}

/// `value.length`: an array's items, a string's UTF-16 units or an object's fields.
pub(crate) fn length_of(value: Value) -> Result<Value, Fail> {
  match value {
    Value::Seq(items) => Ok(Value::F64(items.len() as f64)),
    Value::Str(s) => Ok(Value::F64(s.encode_utf16().count() as f64)),
    Value::Map(map) => Ok(Value::F64(map.len() as f64)),
    other => Err(type_error("length", "an array, a string or an object", &other)),
  }
}

/// The field name a computed key writes: a string's own text, anything else as `String` makes it.
pub(crate) fn key_name(key: Value) -> Result<Name, Fail> {
  match key {
    Value::Str(text) => Ok(text.into_text()),
    other => Ok(Name::new(stringify(&other)?)),
  }
}

pub(crate) fn frozen(value: &Value) -> bool {
  is_frozen(value)
}

pub(super) fn is_frozen(value: &Value) -> bool {
  match value {
    Value::Map(map) => map.is_frozen(),
    Value::Seq(items) => items.is_frozen(),
    _ => false,
  }
}

pub(super) fn independent_lets(stmts: &[Stmt]) -> usize {
  let mut bound: Vec<&str> = Vec::new();
  let mut count = 0;
  for stmt in stmts {
    let Stmt::Let { name, expr } = stmt else { break };
    if makes_function_value(expr) {
      break;
    }
    let mut free = Vec::new();
    expr.free_vars(&mut free);
    if free.iter().any(|f| bound.iter().any(|b| b == f)) {
      break;
    }
    bound.push(name);
    count += 1;
  }
  let calls = stmts[..count].iter().filter(|s| matches!(s, Stmt::Let { expr, .. } if expr.has_call())).count();
  if calls < 2 { 1 } else { count }
}

pub(crate) fn parse_kind(name: &str) -> Option<FailureKind> {
  FailureKind::from_name(name)
}

pub(crate) use snapfire_fsr_core::ext::type_error;

pub(crate) fn truthy(value: &Value) -> bool {
  match value {
    Value::Null | Value::Undefined => false,
    Value::Bool(b) => *b,
    Value::Int(n) => *n != 0,
    Value::UInt(n) => *n != 0,
    Value::F32(f) => *f != 0.0 && !f.is_nan(),
    Value::F64(f) => *f != 0.0 && !f.is_nan(),
    Value::Str(s) => !s.is_empty(),
    _ => true,
  }
}

pub(crate) fn field_of(target: &Value, name: &str) -> Value {
  get_field(target, name)
}

pub(super) fn get_field(target: &Value, name: &str) -> Value {
  match target {
    Value::Map(map) => map.get(name).cloned().unwrap_or(Value::Undefined),
    Value::Variant { tag, .. } if tag == snapfire_fsr_runtime::RESPONSE_TAG && matches!(name, "status" | "ok") => {
      message_builtin(Builtin::ResponseMember, &[target.clone(), Value::str(name)]).unwrap_or(Value::Undefined)
    }
    _ => Value::Undefined,
  }
}

pub(crate) fn index_of(target: &Value, key: &Value) -> Result<Value, Fail> {
  index(target, key)
}

pub(super) fn index(target: &Value, key: &Value) -> Result<Value, Fail> {
  match (target, key) {
    (Value::Map(map), Value::Str(k)) => Ok(map.get(k.as_str()).cloned().unwrap_or(Value::Undefined)),
    (Value::Seq(items), Value::Int(i)) => Ok(usize::try_from(*i).ok().and_then(|i| items.get(i)).cloned().unwrap_or(Value::Undefined)),
    (Value::Seq(items), Value::UInt(i)) => Ok(usize::try_from(*i).ok().and_then(|i| items.get(i)).cloned().unwrap_or(Value::Undefined)),
    (Value::Seq(items), Value::F64(f)) => Ok(if f.fract() == 0.0 && *f >= 0.0 { items.get(*f as usize).cloned().unwrap_or(Value::Undefined) } else { Value::Undefined }),
    (Value::Seq(items), Value::F32(f)) => Ok(if f.fract() == 0.0 && *f >= 0.0 { items.get(*f as usize).cloned().unwrap_or(Value::Undefined) } else { Value::Undefined }),
    (Value::Null | Value::Undefined, _) => Ok(Value::Undefined),
    (Value::Str(s), Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_)) => {
      let at = to_number(key).filter(|n| n.fract() == 0.0 && *n >= 0.0);
      let units: Vec<u16> = s.encode_utf16().collect();
      match at.filter(|n| *n < units.len() as f64) {
        Some(at) => Ok(Value::str(String::from_utf16(&units[at as usize..at as usize + 1]).map_err(|_| Fail::internal("indexing a string would split a surrogate pair, which the value model holds no half of"))?)),
        None => Ok(Value::Undefined),
      }
    }
    (Value::Map(map), Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_)) => Ok(map.get(stringify(key)?.as_str()).cloned().unwrap_or(Value::Undefined)),
    (Value::Map(map), other) => Ok(stringify(other).ok().and_then(|k| map.get(k.as_str()).cloned()).unwrap_or(Value::Undefined)),
    (Value::Seq(items), Value::Str(k)) if k.as_str() == "length" => Ok(Value::F64(items.len() as f64)),
    (Value::Str(s), Value::Str(k)) if k.as_str() == "length" => Ok(Value::F64(s.encode_utf16().count() as f64)),
    (Value::Seq(_) | Value::Str(_), Value::Str(k)) => match k.parse::<u128>() {
      Ok(at) if at.to_string() == k.as_str() => index(target, &Value::UInt(at)),
      _ => Ok(Value::Undefined),
    },
    _ => Ok(Value::Undefined),
  }
}
