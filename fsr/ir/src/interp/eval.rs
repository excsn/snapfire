use super::*;

impl Env {
  /// An expression with a service call somewhere in it suspends on the call;
  /// one without goes through `eval_sync` and never allocates a future.
  pub(crate) fn eval<'a>(&'a mut self, expr: &'a Expr) -> BoxFuture<'a, Result<Value, Fail>> {
    if !expr.has_call() {
      let result = self.eval_sync(expr);
      return Box::pin(async move { result });
    }
    Box::pin(async move {
      match expr {
        Expr::Markup(_) | Expr::After(_) => self.eval_sync(expr),
        Expr::Cell(init) => {
          let value = self.eval(init).await?;
          self.new_cell(value)
        }
        Expr::ModuleCell { key, init } => {
          let key = cell_key(self.eval(key).await?)?;
          if let Some((_, index)) = self.module_cells.iter().find(|(k, _)| *k == key) {
            return Ok(cell_handle(*index));
          }
          let value = match self.module_seed.get(&key) {
            Some(seeded) => seeded.clone(),
            None => self.eval(init).await?,
          };
          let handle = self.new_cell(value)?;
          self.module_cells.push((key, cell_index(&handle).expect("a handle just made")));
          Ok(handle)
        }
        Expr::ReadCell(handle) => {
          let handle = self.eval(handle).await?;
          self.read_cell(&handle)
        }
        Expr::WriteCell(handle, value) => {
          let handle = self.eval(handle).await?;
          let value = self.eval(value).await?;
          self.write_cell(&handle, value)
        }
        Expr::WriteStore(key, value) => {
          let value = self.eval(value).await?;
          Ok(self.write_store(key, value))
        }
        Expr::Const(key) => {
          if let Some(f) = self.functions.get(key).cloned() {
            return self.close_compiled(&f);
          }
          let Some(consts) = self.consts.clone() else { return Err(undeclared(key)) };
          let Some(expr) = consts.get(key) else { return Err(undeclared(key)) };
          self.eval(expr).await
        }
        Expr::Param(name) => Ok(self.read_request(Read::Param(name))),
        Expr::Query(name) => Ok(self.read_request(Read::Query(name))),
      Expr::ParamsAll => Ok(self.read_request(Read::ParamsAll)),
      Expr::QueryAll => Ok(self.read_request(Read::QueryAll)),
        Expr::Session(key) => Ok(self.read_request(Read::Session(key))),
        Expr::Store(key) => Ok(self.store.get(key).cloned().unwrap_or(Value::Null)),
        Expr::Context(id) => Ok(self.contexts.iter().rev().find(|(c, _)| c == id).map(|(_, v)| v.clone()).unwrap_or(Value::Null)),
        Expr::Locale => Ok(self.read_request(Read::Locale)),
        Expr::Locales => Ok(self.read_request(Read::Locales)),
        Expr::Path => Ok(self.read_request(Read::Path)),
        Expr::Document => Ok(self.read_request(Read::Document)),
        Expr::Host => Ok(self.read_request(Read::Host)),
        Expr::Origin => Ok(self.read_request(Read::Origin)),
        Expr::Address => Ok(self.read_request(Read::Address)),
        Expr::Config(key) => Ok(self.read_request(Read::Config(key))),
        Expr::Identity(path) => {
          let mut current = self.identity.clone().unwrap_or(Value::Null);
          for step in path {
            current = get_field(&current, step);
          }
          Ok(current)
        }
        Expr::Input => Ok(self.read_request(Read::Input)),
        Expr::Now => Ok(self.read_request(Read::Now)),
        Expr::Var(name) => self.lookup(name),
        Expr::Folded(value) => Ok(value.clone()),
        Expr::Lit(lit) => Ok(match lit {
          Lit::Null => Value::Null,
          Lit::Undefined => Value::Undefined,
          Lit::Bool(b) => Value::Bool(*b),
          Lit::Int(n) => Value::Int(*n),
          Lit::Float(f) => Value::F64(*f),
          Lit::Str(s) => Value::str(s.clone()),
        }),
        Expr::Object(entries) => {
          let mut map = if matches!(entries.first(), Some(Entry::Spread(_))) { snapfire_fsr_core::Fields::new() } else { snapfire_fsr_core::Fields::with_capacity(entries.len()) };
          for (at, entry) in entries.iter().enumerate() {
            match entry {
              Entry::Baked(_) => {}
              Entry::Field(name, e) => {
                let v = self.eval(e).await?;
                map.insert(name.as_str(), v);
              }
              Entry::Computed(k, e) => {
                let key = key_text(&self.eval(k).await?)?;
                let v = self.eval(e).await?;
                map.insert(key, v);
              }
              Entry::Spread(e) => match self.eval(e).await? {
                Value::Map(inner) if map.is_empty() => map = inner.into_fields_with_room(entries.len() - at - 1),
                Value::Map(inner) => {
                  map.reserve(inner.len());
                  map.extend(inner);
                }
                Value::Null | Value::Undefined => {}
                other => return Err(type_error("spread into an object", "an object", &other)),
              },
              Entry::Item(_) => return Err(Fail::internal("an object literal has no positional entries")),
            }
          }
          Ok(Value::Map(map.into()))
        }
        Expr::Array(entries) => {
          let mut items = Vec::with_capacity(entries.len());
          for entry in entries {
            match entry {
              Entry::Baked(_) => {}
              Entry::Item(e) => items.push(self.eval(e).await?),
              Entry::Spread(e) => match as_items(self.eval(e).await?) {
                Value::Seq(inner) => items.extend(inner),
                Value::Null | Value::Undefined => {}
                Value::Str(text) => items.extend(text.chars().map(|c| Value::str(c.to_string()))),
                other => return Err(type_error("spread into an array", "an array", &other)),
              },
              Entry::Field(..) | Entry::Computed(..) => return Err(Fail::internal("an array literal has no named entries")),
            }
          }
          Ok(Value::seq(items))
        }
        Expr::Field(target, name) => Ok(get_field(&self.eval(target).await?, name)),
        Expr::Index(target, key) => {
          let target = self.eval(target).await?;
          let key = self.eval(key).await?;
          index(&target, &key)
        }
        Expr::Arith(op, l, r) => {
          let l = self.eval(l).await?;
          let r = self.eval(r).await?;
          if converts_itself(&l) || converts_itself(&r) {
            let (l, r) = self.operands(l, r)?;
            return arith(*op, l, r);
          }
          arith(*op, l, r)
        }
        Expr::Compare(op, l, r) => {
          let l = self.eval(l).await?;
          let r = self.eval(r).await?;
          if !matches!(op, CompareOp::Eq | CompareOp::Ne) && (converts_itself(&l) || converts_itself(&r)) {
            let (l, r) = self.operands(l, r)?;
            return compare(*op, &l, &r).map(Value::Bool);
          }
          compare(*op, &l, &r).map(Value::Bool)
        }
        Expr::Logic(op, l, r) => {
          let l = self.eval(l).await?;
          match (op, truthy(&l)) {
            (LogicOp::And, false) | (LogicOp::Or, true) => Ok(l),
            _ => self.eval(r).await,
          }
        }
        Expr::Not(e) => Ok(Value::Bool(!truthy(&self.eval(e).await?))),
        Expr::Coalesce(l, r) => match self.eval(l).await? {
          Value::Null | Value::Undefined => self.eval(r).await,
          v => Ok(v),
        },
        Expr::Ternary(c, t, e) => {
          if truthy(&self.eval(c).await?) {
            self.eval(t).await
          } else {
            self.eval(e).await
          }
        }
        Expr::Template(parts) => {
          let mut out = String::new();
          for part in parts {
            let value = self.eval(part).await?;
            self.text_into(&value, &mut out)?;
          }
          Ok(Value::str(out.as_str()))
        }
        Expr::Call { service, method, args } => {
          let mut map = ValueMap::default();
          for (name, e) in args {
            match self.eval(e).await? {
              Value::Null | Value::Undefined => {}
              v => {
                map.insert(name.as_str(), leaves(self.settled(v))?);
              }
            }
          }
          let answer = self.ctx.services.call(service, method, map).await.map_err(|e| Fail::new(e.kind, e.message).with_call(format!("{service}.{method}")))?;
          // A cached answer is shared with other requests.
          Ok(if self.heap_live { own_copy(&answer) } else { answer })
        }
        Expr::Next(request) => {
          let request = self.eval(request).await?;
          let Some(next) = self.ctx.next.clone() else { return Err(Fail::internal("`next` outside a middleware")) };
          // The route reads the session, so what this body wrote so far has to be there.
          commit(&self.ctx.session, &self.session, &self.written);
          next.call(request).await.map_err(|e| Fail::new(e.kind, e.message))
        }
        Expr::NativeCall { module, method, args, sync } => {
          let mut map = ValueMap::default();
          for (name, e) in args {
            match self.eval(e).await? {
              Value::Null | Value::Undefined => {}
              v => {
                map.insert(name.as_str(), v);
              }
            }
          }
          let _ = sync;
          // A method the build read as `fn` answers here without a future;
          // `sync` on the call site is what the render path will read, since
          // that is the one place a suspension is impossible.
          if let Some(answer) = self.ctx.natives.call_sync(module, method, map.clone()) {
            return answer.map_err(|e| Fail::new(e.kind, e.message));
          }
          self.ctx.natives.call(module, method, map).await.map_err(|e| Fail::new(e.kind, e.message))
        }
        Expr::Lambda { .. } | Expr::Fix { .. } => self.eval_sync(expr),
        Expr::Block(body) => {
          let depth = self.scope.len();
          let flow = self.block(body).await;
          self.scope.truncate(depth);
          flow?.value()
        }
        Expr::Hoist { expr, .. } => self.eval(expr).await,
        Expr::HoistId(id) => self.hoist_id(*id),
        Expr::Handler(_) => Err(Fail::internal("a handler is a value in a render only")),
        Expr::Apply { f, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          self.backing = true;
          let value = self.apply(f, values).await?;
          self.write_back(args)?;
          Ok(value)
        }
        Expr::ApplyOf(f, args) => {
          let values = self.seq(args, "apply").await?;
          self.apply(f, values).await
        }
        Expr::Builtin { name: Builtin::Overwrite, args } if !args[1].has_call() => self.overwrite(&args[0], &args[1]),
        Expr::Builtin { name: Builtin::Overwrite, args } => {
          let held = self.eval(&args[0]).await?;
          let held = self.thawed_for_write(held);
          let next = self.eval(&args[1]).await?;
          self.note_link(&held, &next);
          match (&held, next) {
            (Value::Map(map), Value::Map(next)) => *map.through().ok_or_else(|| Fail::internal("a write to an object every request shares"))? = next.into_fields(),
            (Value::Seq(items), Value::Seq(next)) => *items.through().ok_or_else(|| Fail::internal("a write to an object every request shares"))? = next.into_items(),
            (other, _) => return Err(type_error("a write", "an object or an array", other)),
          }
          Ok(held)
        }
        Expr::Builtin { name: name @ (Builtin::CallMethod | Builtin::GetMember), args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          let (target, member) = (values.first().cloned().unwrap_or(Value::Undefined), match values.get(1) { Some(Value::Str(name)) => name.to_string(), _ => String::new() });
          let getter = *name == Builtin::GetMember;
          match instance_class(&target).and_then(|class| class_member(self.consts.as_deref(), &class, &member, getter)) {
            Some(key) => {
              let mut called = vec![target];
              called.extend(values.into_iter().skip(2));
              self.apply(&Expr::Const(key), called).await
            }
            None if getter => Ok(get_field(&target, &member)),
            None => Err(Fail::internal(format!("`{member}` is not a method of {}", instance_class(&target).unwrap_or_else(|| "this value".to_owned())))),
          }
        }
        Expr::Builtin { name, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          match name {
            Builtin::MapKey | Builtin::MapOf => self.keyed(*name, values),
            Builtin::Join => self.join(values),
            Builtin::LooseEq if values.iter().any(converts_itself) => {
              let mut values = values.into_iter();
              let (l, r) = (values.next().unwrap_or(Value::Undefined), values.next().unwrap_or(Value::Undefined));
              let (l, r) = if matches!(l, Value::Map(_)) && matches!(r, Value::Map(_)) { (l, r) } else { self.operands(l, r)? };
              loose_equal(&l, &r).map(Value::Bool)
            }
            _ => builtin(*name, values),
          }
        }
        Expr::Ext { module, name, args } => {
          let mut values = Vec::with_capacity(args.len());
          for arg in args {
            values.push(self.eval(arg).await?);
          }
          self.extensions.call(&format!("{module}.{name}"), &self.ambient(), &values)
        }
        Expr::Map(over, f) => {
          let items = self.seq(over, "map").await?; let whole = array_arg(f, &items);
          let mut out = Vec::with_capacity(items.len());
          for (i, item) in items.into_iter().enumerate() {
            out.push(self.apply(f, with_array(vec![item, Value::F64(i as f64)], &whole)).await?);
          }
          Ok(Value::seq(out))
        }
        Expr::ReplaceWith(subject, pattern, f) => {
          let subject = self.eval(subject).await?;
          let pattern = self.eval(pattern).await?;
          let (s, found) = replacements(&subject, &pattern)?;
          let mut out = String::with_capacity(s.len());
          let mut last = 0;
          for (start, end, args) in found {
            out.push_str(&s[last..start]);
            out.push_str(&stringify(&self.apply(f, args).await?)?);
            last = end;
          }
          out.push_str(&s[last..]);
          Ok(Value::str(out))
        }
        Expr::FlatMap(over, f) => {
          let items = self.seq(over, "flatMap").await?; let whole = array_arg(f, &items);
          let mut out = Vec::with_capacity(items.len());
          for (i, item) in items.into_iter().enumerate() {
            match self.apply(f, with_array(vec![item, Value::F64(i as f64)], &whole)).await? {
              Value::Seq(inner) => out.extend(inner.into_items()),
              other => out.push(other),
            }
          }
          Ok(Value::seq(out))
        }
        Expr::Sort(over, f) => {
          let mut items = self.seq(over, "toSorted").await?;
          let mut width = 1;
          while width < items.len() {
            let mut merged = Vec::with_capacity(items.len());
            let mut start = 0;
            while start < items.len() {
              let mid = (start + width).min(items.len());
              let end = (start + 2 * width).min(items.len());
              let (mut a, mut b) = (start, mid);
              while a < mid && b < end {
                let order = match &**f {
                  Expr::Lit(Lit::Null) | Expr::Folded(Value::Null) => default_order(&items[a], &items[b])?,
                  f => comparator_order(&self.apply(f, vec![items[a].clone(), items[b].clone()]).await?),
                };
                if order > 0.0 {
                  merged.push(items[b].clone());
                  b += 1;
                } else {
                  merged.push(items[a].clone());
                  a += 1;
                }
              }
              merged.extend_from_slice(&items[a..mid]);
              merged.extend_from_slice(&items[b..end]);
              start = end;
            }
            items = merged;
            width *= 2;
          }
          Ok(Value::seq(items))
        }
        Expr::Filter(over, f) => {
          let items = self.seq(over, "filter").await?; let whole = array_arg(f, &items);
          let mut out = Vec::new();
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, with_array(vec![item.clone(), Value::F64(i as f64)], &whole)).await?) {
              out.push(item);
            }
          }
          Ok(Value::seq(out))
        }
        Expr::Reduce(over, init, f) => {
          let items = self.seq(over, "reduce").await?; let whole = array_arg(f, &items);
          let mut acc = self.eval(init).await?;
          for (i, item) in items.into_iter().enumerate() {
            acc = self.apply(f, with_array(vec![acc, item, Value::F64(i as f64)], &whole)).await?;
          }
          Ok(acc)
        }
        Expr::Find(over, f) => {
          let items = self.seq(over, "find").await?; let whole = array_arg(f, &items);
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, with_array(vec![item.clone(), Value::F64(i as f64)], &whole)).await?) {
              return Ok(item);
            }
          }
          Ok(Value::Undefined)
        }
        Expr::FindIndex(over, f) => {
          let items = self.seq(over, "findIndex").await?; let whole = array_arg(f, &items);
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, with_array(vec![item, Value::F64(i as f64)], &whole)).await?) {
              return Ok(Value::F64(i as f64));
            }
          }
          Ok(Value::F64(-1.0))
        }
        Expr::Some(over, f) => {
          let items = self.seq(over, "some").await?; let whole = array_arg(f, &items);
          for (i, item) in items.into_iter().enumerate() {
            if truthy(&self.apply(f, with_array(vec![item, Value::F64(i as f64)], &whole)).await?) {
              return Ok(Value::Bool(true));
            }
          }
          Ok(Value::Bool(false))
        }
        Expr::Every(over, f) => {
          let items = self.seq(over, "every").await?; let whole = array_arg(f, &items);
          for (i, item) in items.into_iter().enumerate() {
            if !truthy(&self.apply(f, with_array(vec![item, Value::F64(i as f64)], &whole)).await?) {
              return Ok(Value::Bool(false));
            }
          }
          Ok(Value::Bool(true))
        }
        Expr::Entries(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(
            map.into_iter().map(|(k, v)| Value::seq(vec![self.key_value(k.to_string()), v])).collect(),
          )),
          other => Err(type_error("Object.entries", "an object", &other)),
        },
        Expr::Keys(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(map.into_fields().into_keys().map(|k| self.key_value(k.to_string())).collect())),
          other => Err(type_error("Object.keys", "an object", &other)),
        },
        Expr::Values(e) => match self.eval(e).await? {
          Value::Map(map) => Ok(Value::Seq(map.into_fields().into_values().collect())),
          other => Err(type_error("Object.values", "an object", &other)),
        },
        Expr::Length(e) => match self.eval(e).await? {
          Value::Seq(items) => Ok(Value::F64(items.len() as f64)),
          Value::Str(s) => Ok(Value::F64(s.encode_utf16().count() as f64)),
          Value::Map(map) => Ok(Value::F64(map.len() as f64)),
          other => length_of(other),
        },
        Expr::Str(e) => {
          let value = self.eval(e).await?;
          spelled(|out| self.text_into(&value, out))
        }
        Expr::Num(e) => number_of(self.eval(e).await?),
        Expr::BigInt(e) => bigint_of(self.eval(e).await?),
      }
    })
  }

  pub(super) async fn seq(&mut self, over: &Expr, what: &str) -> Result<Vec<Value>, Fail> {
    match as_items(self.eval(over).await?) {
      Value::Seq(items) => Ok(items.into_items()),
      other => Err(type_error(what, "an array", &other)),
    }
  }

  pub(crate) async fn apply(&mut self, f: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let Expr::Lambda { params, body } = f else {
      return self.apply_sync(f, args);
    };
    let backing = std::mem::take(&mut self.backing);
    let depth = self.scope.len();
    bind_params(&mut self.scope, params, args);
    let result = self.eval(body).await;
    self.scope.truncate(depth);
    self.unwrap_written(result, backing)
  }
}
