use super::*;

impl Env {
  /// A function value of a prepared function, what its body reads taken now. One whose captures are not all bound here is made by [`Self::close`], which leaves them out.
  pub(crate) fn close_compiled(&mut self, f: &Arc<crate::compiled::Function>) -> Result<Value, Fail> {
    let mut scope = Vec::with_capacity(f.captures.len());
    for name in &f.captures {
      match self.scope.iter().rev().find(|(n, _)| n == name) {
        Some((_, value)) => scope.push((Name::new(name), value.clone())),
        None => return self.close(&f.params, &f.body, f.name.clone()),
      }
    }
    let closure = Closure { params: f.params.clone(), body: f.body.clone(), scope, name: f.name.clone(), compiled: Some(f.clone()) };
    let Some(table) = &mut self.closures else {
      return Err(Fail::internal("a function value made while the body's independent lets evaluate in parallel"));
    };
    table.push(closure);
    Ok(Value::Variant { tag: FUNCTION_VALUE.to_owned(), payload: Some(Box::new(Value::Int((table.len() - 1) as i128))) })
  }

  /// A lambda written where a value goes, as a function value: what its body reads is taken now.
  pub(super) fn close(&mut self, params: &[String], body: &Expr, name: Option<String>) -> Result<Value, Fail> {
    let mut names = Vec::new();
    body.free_vars(&mut names);
    let scope = names.into_iter().filter(|name| !params.contains(name)).filter_map(|name| self.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| (Name::new(name), v.clone()))).collect();
    let closure = Closure { params: params.to_vec(), body: std::sync::Arc::new(body.clone()), scope, name, compiled: None };
    let Some(table) = &mut self.closures else {
      return Err(Fail::internal("a function value made while the body's independent lets evaluate in parallel"));
    };
    table.push(closure);
    Ok(Value::Variant { tag: FUNCTION_VALUE.to_owned(), payload: Some(Box::new(Value::Int((table.len() - 1) as i128))) })
  }

  /// `value` written to the store key for the rest of the step and kept for the step to return.
  pub(super) fn write_store(&mut self, key: &str, value: Value) -> Value {
    self.set_store(key, Some(value.clone()));
    self.store_written.insert(key.to_owned(), value.clone());
    value
  }

  pub(super) fn new_cell(&mut self, value: Value) -> Result<Value, Fail> {
    let Some(cells) = &mut self.cells else {
      return Err(Fail::internal("a cell made while the body's independent lets evaluate in parallel"));
    };
    cells.push(value);
    Ok(cell_handle(cells.len() - 1))
  }

  pub(super) fn read_cell(&self, handle: &Value) -> Result<Value, Fail> {
    cell_index(handle).and_then(|index| self.cells.as_ref().and_then(|cells| cells.get(index))).cloned().ok_or_else(|| type_error("a cell read", "a cell", handle))
  }

  pub(super) fn write_cell(&mut self, handle: &Value, value: Value) -> Result<Value, Fail> {
    let slot = cell_index(handle).and_then(|index| self.cells.as_mut().and_then(|cells| cells.get_mut(index)));
    let Some(slot) = slot else { return Err(type_error("a cell write", "a cell", handle)) };
    *slot = value.clone();
    Ok(value)
  }

  /// The function value `f` evaluates to, for a call whose target is not a lambda written in place.
  pub(super) fn closure_of(&mut self, f: &Expr) -> Result<(Value, Closure), Fail> {
    let value = self.eval_sync(f)?;
    let closure = closure_index(&value).and_then(|index| self.closures.as_ref().and_then(|table| table.get(index)).cloned()).ok_or_else(|| type_error("call", "a function", &value))?;
    Ok((value, closure))
  }

  /// A callback of an array method applied to an item and its index, with no list of arguments when it is a lambda written in place.
  #[inline]
  pub(super) fn apply_item(&mut self, f: &Expr, item: Value, index: usize, whole: &Option<Value>) -> Result<Value, Fail> {
    if let Expr::Lambda { params, body } = f {
      if params.len() <= 2 && !params.iter().any(|p| p.starts_with("...")) {
        let backing = std::mem::take(&mut self.backing);
        let depth = self.scope.len();
        let mut params = params.iter();
        if let Some(first) = params.next() {
          self.scope.push((Name::new(first), item));
        }
        if let Some(second) = params.next() {
          self.scope.push((Name::new(second), Value::F64(index as f64)));
        }
        let result = self.eval_sync(body);
        self.scope.truncate(depth);
        return self.unwrap_written(result, backing);
      }
    }
    self.apply_sync(f, with_array(vec![item, Value::F64(index as f64)], whole))
  }

  pub(crate) fn apply_sync(&mut self, f: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let backing = std::mem::take(&mut self.backing);
    if !matches!(f, Expr::Lambda { .. }) {
      if let Some(token) = self.eval_sync(f).ok().as_ref().and_then(crate::render::handler_token) {
        self.handler_calls.push((token.to_owned(), args.into_iter().next().unwrap_or(Value::Null)));
        return Ok(Value::Null);
      }
    }
    let Expr::Lambda { params, body } = f else {
      let (value, closure) = match self.closure_of(f) {
        Ok(found) => found,
        Err(_) if self.in_framework => {
          self.unrendered = true;
          return Ok(Value::Null);
        }
        Err(e) => return Err(e),
      };
      if self.depth == MAX_APPLIES {
        let who = closure.name.as_deref().map(|name| format!("`{name}`")).unwrap_or_else(|| "a function".to_owned());
        return Err(Fail::internal(format!("{who} called itself {MAX_APPLIES} levels deep, which a render refuses: a function that calls itself needs a case that calls no further")));
      }
      let mut scope = closure.scope;
      if let Some(name) = &closure.name {
        scope.push((Name::new(name), value));
      }
      bind_params(&mut scope, &closure.params, args);
      let outer = std::mem::replace(&mut self.scope, scope);
      self.depth += 1;
      let result = on_enough_stack(|| match &closure.compiled {
        Some(f) => f.run(self),
        None => self.eval_sync(&closure.body),
      });
      self.depth -= 1;
      self.scope = outer;
      return self.unwrap_written(result, backing);
    };
    let depth = self.scope.len();
    bind_params(&mut self.scope, params, args);
    let result = self.eval_sync(body);
    self.scope.truncate(depth);
    self.unwrap_written(result, backing)
  }

  /// A function's answer with what it changed of its parameters, [`Builtin::WriteBack`], held for the call to write back. Applied where nothing takes the change back, a callback of `map` or `forEach` among them, it fails: JavaScript would have changed the value in place.
  pub(super) fn unwrap_written(&mut self, result: Result<Value, Fail>, backing: bool) -> Result<Value, Fail> {
    self.written_back = None;
    let value = result?;
    let Value::Map(map) = &value else { return Ok(value) };
    let Some(Value::Seq(parts)) = map.get(crate::render::WRITE_BACK) else { return Ok(value) };
    if !backing {
      return Err(Fail::internal("a function that writes a field of what it is passed, called where the change cannot reach the caller's value, as a callback of an array method is; return a new value instead"));
    }
    let (Some(Value::Seq(packed)), Some(Value::Seq(indexes))) = (parts.first(), parts.get(1)) else { return Ok(value) };
    let Some((answer, lasts)) = packed.split_first() else { return Ok(value) };
    let mut held = Vec::with_capacity(lasts.len());
    for (index, last) in indexes.iter().zip(lasts) {
      let index = match index {
        Value::Int(i) => usize::try_from(*i).ok(),
        Value::F64(f) if *f >= 0.0 && f.fract() == 0.0 => Some(*f as usize),
        _ => None,
      };
      if let Some(index) = index {
        held.push((index, last.clone()));
      }
    }
    let answer = answer.clone();
    self.written_back = Some(held);
    Ok(answer)
  }

  /// Writes what the function just applied changed of its parameters back to the arguments that are a variable, a cell or a path from one. The same place passed for two changed parameters fails, since JavaScript would see one object changed twice.
  pub(super) fn write_back(&mut self, args: &[Expr]) -> Result<(), Fail> {
    let Some(written) = self.written_back.take() else { return Ok(()) };
    let mut places: Vec<(Place, Vec<Value>)> = Vec::new();
    for (index, last) in written {
      let Some(arg) = args.get(index) else { continue };
      let Some((root, steps)) = self.place_of(arg)? else { continue };
      if places.iter().any(|(r, s)| *r == root && *s == steps) {
        let named = match &root {
          Place::Var(name) => format!("`{name}`"),
          Place::Cell(_) => "one value".to_owned(),
        };
        return Err(Fail::internal(format!("{named} passed twice to a function that changes both parameters it takes it as")));
      }
      match &root {
        Place::Var(name) => {
          if let Some(at) = self.scope.iter().rposition(|(n, _)| n == name) {
            set_path(&mut self.scope[at].1, &steps, last)?;
          }
        }
        Place::Cell(handle) => {
          let mut held = self.read_cell(handle)?;
          set_path(&mut held, &steps, last)?;
          self.write_cell(handle, held)?;
        }
      }
      places.push((root, steps));
    }
    Ok(())
  }

  /// `arg` as the variable or cell it reads and the fields and indexes from it, `None` when it is not one.
  pub(super) fn place_of(&mut self, arg: &Expr) -> Result<Option<(Place, Vec<Value>)>, Fail> {
    Ok(match arg {
      Expr::Var(name) => Some((Place::Var(name.clone()), Vec::new())),
      Expr::ReadCell(handle) => Some((Place::Cell(self.eval_sync(handle)?), Vec::new())),
      Expr::Field(on, field) => self.place_of(on)?.map(|(root, mut steps)| {
        steps.push(Value::str(field.clone()));
        (root, steps)
      }),
      Expr::Index(on, at) => match self.place_of(on)? {
        Some((root, mut steps)) => {
          steps.push(self.eval_sync(at)?);
          Some((root, steps))
        }
        None => None,
      },
      _ => None,
    })
  }
}
