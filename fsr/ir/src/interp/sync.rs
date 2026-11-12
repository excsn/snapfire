use super::*;

impl Env {
  #[inline]
  pub(crate) fn eval_sync(&mut self, expr: &Expr) -> Result<Value, Fail> {
    if !self.heap_live {
      return self.eval_node(expr);
    }
    if matches!(expr, Expr::Session(_) | Expr::Store(_) | Expr::Config(_) | Expr::Context(_) | Expr::Input | Expr::Identity(_)) {
      if let Some(value) = self.own_read(expr)? {
        return Ok(value);
      }
    }
    if self.thawed.is_empty() {
      return self.eval_node(expr);
    }
    let value = self.eval_node(expr)?;
    Ok(self.resolved(value))
  }

  pub(crate) fn eval_node(&mut self, expr: &Expr) -> Result<Value, Fail> {
    if matches!(expr, Expr::Field(..) | Expr::Index(..)) {
      if let Some(value) = self.place(expr) {
        return Ok(value.clone());
      }
    }
    match expr {
      Expr::Markup(tmpl) => self.make(tmpl),
      Expr::Cell(init) => {
        let value = self.eval_sync(init)?;
        self.new_cell(value)
      }
      Expr::ModuleCell { key, init } => {
        let key = cell_key(self.eval_sync(key)?)?;
        if let Some((_, index)) = self.module_cells.iter().find(|(k, _)| *k == key) {
          return Ok(cell_handle(*index));
        }
        let value = match self.module_seed.get(&key) {
          Some(seeded) => seeded.clone(),
          None => self.eval_sync(init)?,
        };
        let handle = self.new_cell(value)?;
        self.module_cells.push((key, cell_index(&handle).expect("a handle just made")));
        Ok(handle)
      }
      Expr::ReadCell(handle) => {
        let handle = self.eval_sync(handle)?;
        self.read_cell(&handle)
      }
      Expr::WriteCell(handle, value) => {
        let handle = self.eval_sync(handle)?;
        let value = self.eval_sync(value)?;
        self.write_cell(&handle, value)
      }
      Expr::WriteStore(key, value) => {
        let value = self.eval_sync(value)?;
        Ok(self.write_store(key, value))
      }
      Expr::Const(key) => self.const_value(key),
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
      Expr::Object(entries) => self.object_sync(entries),
      Expr::Array(entries) => self.array_sync(entries),
      Expr::Field(target, name) => Ok(get_field(&self.eval_sync(target)?, name)),
      Expr::Index(target, key) => {
        if self.place(target).is_some() {
          let key = self.eval_sync(key)?;
          if let Some(target) = self.place(target) {
            return index(target, &key);
          }
        }
        let target = self.eval_sync(target)?;
        let key = self.eval_sync(key)?;
        index(&target, &key)
      }
      Expr::Arith(op, l, r) => {
        let l = self.eval_sync(l)?;
        let r = self.eval_sync(r)?;
        if converts_itself(&l) || converts_itself(&r) {
          let (l, r) = self.operands(l, r)?;
          return arith(*op, l, r);
        }
        arith(*op, l, r)
      }
      Expr::Compare(op, l, r) => {
        let relational = !matches!(op, CompareOp::Eq | CompareOp::Ne);
        if let (Some(lv), Some(rv)) = (self.place(l), self.place(r)) {
          if !relational || !(converts_itself(lv) || converts_itself(rv)) {
            return compare(*op, lv, rv).map(Value::Bool);
          }
        }
        let l = self.eval_sync(l)?;
        let r = self.eval_sync(r)?;
        if relational && (converts_itself(&l) || converts_itself(&r)) {
          let (l, r) = self.operands(l, r)?;
          return compare(*op, &l, &r).map(Value::Bool);
        }
        compare(*op, &l, &r).map(Value::Bool)
      }
      Expr::Logic(op, l, r) => {
        if let Some(lv) = self.place(l) {
          return match (op, truthy(lv)) {
            (LogicOp::And, false) | (LogicOp::Or, true) => Ok(lv.clone()),
            _ => self.eval_sync(r),
          };
        }
        let l = self.eval_sync(l)?;
        match (op, truthy(&l)) {
          (LogicOp::And, false) | (LogicOp::Or, true) => Ok(l),
          _ => self.eval_sync(r),
        }
      }
      Expr::Not(e) => Ok(Value::Bool(!self.truth_of(e)?)),
      Expr::Coalesce(l, r) => match self.eval_sync(l)? {
        Value::Null | Value::Undefined => self.eval_sync(r),
        v => Ok(v),
      },
      Expr::Ternary(c, t, e) => {
        if self.truth_of(c)? {
          self.eval_sync(t)
        } else {
          self.eval_sync(e)
        }
      }
      Expr::Template(parts) => spelled(|out| {
        for part in parts {
          let mark = out.len();
          let plain = match self.place(part) {
            Some(value) => plain_text_into(value, out)?,
            None => false,
          };
          if !plain {
            out.truncate(mark);
            let value = self.eval_sync(part)?;
            self.text_into(&value, out)?;
          }
        }
        Ok(())
      }),
      Expr::Call { .. } => Err(Fail::internal("a service call in an expression that cannot suspend")),
      Expr::Next(_) => Err(Fail::internal("`next` in an expression that cannot suspend")),
      Expr::After(work) => {
        if !matches!(**work, Expr::Lambda { .. }) {
          return Err(Fail::internal("`after` takes a function"));
        }
        let mut later = self.snapshot();
        let work = (**work).clone();
        self.ctx.after.push(async move {
          if let Err(fail) = later.apply(&work, Vec::new()).await {
            tracing::warn!(target: "fsr::after", kind = fail.kind.as_str(), message = %fail.message, "work after the response failed");
          }
        });
        Ok(Value::Undefined)
      }
      Expr::NativeCall { module, method, args, sync } => self.native_sync(module, method, args, *sync),
      Expr::Lambda { params, body } => self.close(params, body, None),
      Expr::Fix { name, f } => match &**f {
        Expr::Lambda { params, body } => self.close(params, body, Some(name.clone())),
        _ => Err(Fail::internal("`fix` takes a function")),
      },
      Expr::Block(body) => {
        let depth = self.scope.len();
        let flow = self.block_sync(body);
        self.scope.truncate(depth);
        flow?.value()
      }
      Expr::Handler(index) => Ok(crate::render::handler_value(&self.component_path, *index)),
      Expr::Hoist { id, expr } => {
        let value = self.eval_sync(expr)?;
        if let Some(hoists) = self.hoists.as_mut().filter(|_| !crate::render::holds_made(&value)) {
          hoists.record(*id, value.clone());
        }
        Ok(value)
      }
      Expr::HoistId(id) => self.hoist_id(*id),
      Expr::Apply { f, args } => {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
          values.push(self.eval_sync(arg)?);
        }
        self.backing = true;
        let value = self.apply_sync(f, values)?;
        self.write_back(args)?;
        Ok(value)
      }
      Expr::ApplyOf(f, args) => {
        let values = self.seq_sync(args, "apply")?;
        self.apply_sync(f, values)
      }
      Expr::Builtin { name, args } => self.builtin_sync(name, args),
      Expr::Ext { module, name, args } => {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
          values.push(self.eval_sync(arg)?);
        }
        self.extensions.call(&format!("{module}.{name}"), &self.ambient(), &values)
      }
      Expr::Map(over, f) => {
        let items = self.seq_sync(over, "map")?; let whole = array_arg(f, &items);
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
          out.push(self.apply_item(f, item, i, &whole)?);
        }
        Ok(Value::seq(out))
      }
      Expr::ReplaceWith(subject, pattern, f) => self.replace_sync(subject, pattern, f),
      Expr::FlatMap(over, f) => {
        let items = self.seq_sync(over, "flatMap")?; let whole = array_arg(f, &items);
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
          match self.apply_item(f, item, i, &whole)? {
            Value::Seq(inner) => out.extend(inner.into_items()),
            other => out.push(other),
          }
        }
        Ok(Value::seq(out))
      }
      Expr::Sort(over, f) => self.sort_sync(over, f),
      Expr::Filter(over, f) => {
        let items = self.seq_sync(over, "filter")?; let whole = array_arg(f, &items);
        let mut out = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_item(f, item.clone(), i, &whole)?) {
            out.push(item);
          }
        }
        Ok(Value::seq(out))
      }
      Expr::Reduce(over, init, f) => {
        let items = self.seq_sync(over, "reduce")?; let whole = array_arg(f, &items);
        let mut acc = self.eval_sync(init)?;
        for (i, item) in items.into_iter().enumerate() {
          acc = self.apply_sync(f, with_array(vec![acc, item, Value::F64(i as f64)], &whole))?;
        }
        Ok(acc)
      }
      Expr::Find(over, f) => {
        let items = self.seq_sync(over, "find")?; let whole = array_arg(f, &items);
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_item(f, item.clone(), i, &whole)?) {
            return Ok(item);
          }
        }
        Ok(Value::Undefined)
      }
      Expr::FindIndex(over, f) => {
        let items = self.seq_sync(over, "findIndex")?; let whole = array_arg(f, &items);
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_item(f, item, i, &whole)?) {
            return Ok(Value::F64(i as f64));
          }
        }
        Ok(Value::F64(-1.0))
      }
      Expr::Some(over, f) => {
        let items = self.seq_sync(over, "some")?; let whole = array_arg(f, &items);
        for (i, item) in items.into_iter().enumerate() {
          if truthy(&self.apply_item(f, item, i, &whole)?) {
            return Ok(Value::Bool(true));
          }
        }
        Ok(Value::Bool(false))
      }
      Expr::Every(over, f) => {
        let items = self.seq_sync(over, "every")?; let whole = array_arg(f, &items);
        for (i, item) in items.into_iter().enumerate() {
          if !truthy(&self.apply_item(f, item, i, &whole)?) {
            return Ok(Value::Bool(false));
          }
        }
        Ok(Value::Bool(true))
      }
      Expr::Entries(e) => match self.eval_sync(e)? {
        Value::Map(map) => Ok(Value::Seq(
          map.into_iter().filter(|(k, _)| !snapfire_fsr_core::value::typed::symbol_key(k)).map(|(k, v)| Value::seq(vec![self.key_value(k.to_string()), v])).collect(),
        )),
        other => Err(type_error("Object.entries", "an object", &other)),
      },
      Expr::Keys(e) => {
        let value = self.eval_sync(e)?;
        self.keys_of(value)
      }
      Expr::Values(e) => values_of(self.eval_sync(e)?),
      Expr::Length(e) => length_of(self.eval_sync(e)?),
      Expr::Str(e) => {
        let value = self.eval_sync(e)?;
        spelled(|out| self.text_into(&value, out))
      }
      Expr::Num(e) => number_of(self.eval_sync(e)?),
      Expr::BigInt(e) => bigint_of(self.eval_sync(e)?),
    }
  }
  /// A literal object under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn object_sync(&mut self, entries: &[Entry]) -> Result<Value, Fail> {
    let mut map = if matches!(entries.first(), Some(Entry::Spread(_))) { snapfire_fsr_core::Fields::new() } else { snapfire_fsr_core::Fields::with_capacity(entries.len()) };
    for (at, entry) in entries.iter().enumerate() {
      match entry {
        Entry::Baked(_) => {}
        Entry::Field(name, e) => {
          let v = self.eval_sync(e)?;
          map.insert(name.as_str(), v);
        }
        Entry::Computed(k, e) => {
          let key = key_text(&self.eval_sync(k)?)?;
          let v = self.eval_sync(e)?;
          map.insert(key, v);
        }
        // A first spread's keys are distinct already, so its fields are taken whole.
        Entry::Spread(e) => match self.eval_sync(e)? {
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

  /// A literal array under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn array_sync(&mut self, entries: &[Entry]) -> Result<Value, Fail> {
    let mut items = Vec::with_capacity(entries.len());
    for entry in entries {
      match entry {
        Entry::Baked(_) => {}
        Entry::Item(e) => items.push(self.eval_sync(e)?),
        Entry::Spread(e) => match as_items(self.eval_sync(e)?) {
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

  /// A native call under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn native_sync(&mut self, module: &str, method: &str, args: &[(String, Expr)], sync: bool) -> Result<Value, Fail> {
    if !sync {
      return Err(Fail::internal("an async native call in an expression that cannot suspend"));
    }
    let mut map = ValueMap::default();
    for (name, e) in args {
      match self.eval_sync(e)? {
        Value::Null | Value::Undefined => {}
        v => {
          map.insert(name.as_str(), v);
        }
      }
    }
    match self.ctx.natives.call_sync(module, method, map) {
      Some(answer) => answer.map_err(|e| Fail::new(e.kind, e.message)),
      None => Err(Fail::internal(format!("`{module}.{method}` is not a synchronous native"))),
    }
  }

  /// A builtin call under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn builtin_sync(&mut self, name: &Builtin, args: &[Expr]) -> Result<Value, Fail> {
    match name {
      Builtin::HeapOn => {
        self.heap_live = true;
        return Ok(Value::Null);
      }
      Builtin::Overwrite => return self.overwrite(&args[0], &args[1]),
      _ => {}
    }
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
      values.push(self.eval_sync(arg)?);
    }
    if *name == Builtin::Json && self.heap_live && values.first().is_some_and(|value| value.nested_past(MAX_NESTING)) {
      return Err(Fail::new(FailureKind::Invalid, "Converting circular structure to JSON"));
    }
    self.builtin_values(name, values)
  }

  /// A builtin applied to its arguments, evaluated already: what [`Self::builtin_sync`] does once it has them.
  pub(crate) fn builtin_values(&mut self, name: &Builtin, values: Vec<Value>) -> Result<Value, Fail> {
    match name {
      Builtin::CloneElement => self.clone_element(values),
      Builtin::CallMethod | Builtin::GetMember => {
        let (target, member) = (values.first().cloned().unwrap_or(Value::Undefined), match values.get(1) { Some(Value::Str(name)) => name.to_string(), _ => String::new() });
        let getter = *name == Builtin::GetMember;
        match instance_class(&target).and_then(|class| class_member(self.consts.as_deref(), &class, &member, getter)) {
          Some(key) => {
            let mut called = vec![target];
            called.extend(values.into_iter().skip(2));
            self.apply_sync(&Expr::Const(key), called)
          }
          None if getter => Ok(get_field(&target, &member)),
          None => Err(Fail::internal(format!("`{member}` is not a method of {}", instance_class(&target).unwrap_or_else(|| "this value".to_owned())))),
        }
      }
      Builtin::Emit => {
        if let (true, Some(Value::Str(kind)), Some(value)) = (self.server_mode, values.first(), values.get(1)) {
          self.emitted.push(crate::render::Emitted { path: self.component_path.clone(), kind: kind.to_string(), value: value.clone() });
        }
        Ok(Value::Null)
      }
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

  /// `value` as a primitive, as JavaScript's `ToPrimitive` makes one: the `toString` and `valueOf` of an object or of its class, called in the order the hint gives, `toString` first for a string. Anything that is not an object which converts itself comes back as it is.
  pub(super) fn primitive(&mut self, value: Value, string_first: bool) -> Result<Value, Fail> {
    if !converts_itself(&value) {
      return Ok(value);
    }
    let order = if string_first { ["toString", "valueOf"] } else { ["valueOf", "toString"] };
    for method in order {
      let own = match &value {
        Value::Map(map) => map.get(method).filter(|f| closure_index(f).is_some()).cloned(),
        _ => None,
      };
      let result = match own {
        Some(f) => self.apply_sync(&Expr::Folded(f), Vec::new())?,
        None => match instance_class(&value).and_then(|class| class_member(self.consts.as_deref(), &class, method, false)) {
          Some(key) => self.apply_sync(&Expr::Const(key), vec![value.clone()])?,
          None => continue,
        },
      };
      if !matches!(result, Value::Map(_) | Value::Seq(_)) {
        return Ok(result);
      }
    }
    Ok(Value::str("[object Object]"))
  }

  /// `String(value)` onto the end of `out`, through [`Self::primitive`] for an object that converts itself, inside an array too.
  pub(crate) fn text_into(&mut self, value: &Value, out: &mut String) -> Result<(), Fail> {
    let mark = out.len();
    if plain_text_into(value, out)? {
      return Ok(());
    }
    out.truncate(mark);
    match value {
      Value::Seq(items) => {
        let items = items.clone();
        on_enough_stack(|| {
          for (at, item) in items.iter().enumerate() {
            if at > 0 {
              out.push(',');
            }
            if !matches!(item, Value::Null | Value::Undefined) {
              self.text_into(item, out)?;
            }
          }
          Ok(())
        })
      }
      other => {
        let primitive = self.primitive(other.clone(), true)?;
        stringify_into(&primitive, out)
      }
    }
  }

  /// `l` and `r` as an operator that converts objects reads them: each object that converts itself through [`Self::primitive`].
  pub(super) fn operands(&mut self, l: Value, r: Value) -> Result<(Value, Value), Fail> {
    Ok((self.primitive(l, false)?, self.primitive(r, false)?))
  }

  /// `Join`, each item read through [`Self::text_into`].
  pub(super) fn join(&mut self, values: Vec<Value>) -> Result<Value, Fail> {
    let Some(Value::Seq(items)) = values.first() else { return builtin(Builtin::Join, values) };
    let sep = match values.get(1) {
      Some(v) => stringify(v)?,
      None => ",".to_owned(),
    };
    let items = items.clone();
    spelled(|out| {
      for (at, item) in items.iter().enumerate() {
        if at > 0 {
          out.push_str(&sep);
        }
        if !matches!(item, Value::Null | Value::Undefined) {
          self.text_into(item, out)?;
        }
      }
      Ok(())
    })
  }

  /// A module constant: held once for the process when it is pure, a function value of its prepared function, otherwise evaluated as written.
  pub(crate) fn const_value(&mut self, key: &str) -> Result<Value, Fail> {
    if let Some(value) = self.held.get(key) {
      return Ok(value.clone());
    }
    if let Some(f) = self.functions.get(key).cloned() {
      return self.close_compiled(&f);
    }
    let Some(consts) = self.consts.clone() else { return Err(undeclared(key)) };
    let Some(expr) = consts.get(key) else { return Err(undeclared(key)) };
    let value = self.eval_sync(expr)?;
    self.held.hold(key, &value);
    Ok(self.held.get(key).cloned().unwrap_or(value))
  }

  /// `Object.keys(value)`, a `Map`'s object keys read back as the objects they are.
  pub(crate) fn keys_of(&self, value: Value) -> Result<Value, Fail> {
    match value {
      Value::Map(map) => Ok(Value::Seq(map.into_fields().into_keys().filter(|k| !snapfire_fsr_core::value::typed::symbol_key(k)).map(|k| self.key_value(k.to_string())).collect())),
      other => Err(type_error("Object.keys", "an object", &other)),
    }
  }

  pub(crate) fn heap_on(&mut self) {
    self.heap_live = true;
  }

  pub(crate) fn thawed_is_empty(&self) -> bool {
    self.thawed.is_empty()
  }

  pub(crate) fn resolved_value(&self, value: Value) -> Value {
    self.resolved(value)
  }

  pub(crate) fn set_backing(&mut self) {
    self.backing = true;
  }

  pub(crate) fn write_back_args(&mut self, args: &[Expr]) -> Result<(), Fail> {
    self.write_back(args)
  }

  pub(crate) fn take_backing(&mut self) -> bool {
    std::mem::take(&mut self.backing)
  }

  pub(crate) fn unwrap_written_value(&mut self, result: Result<Value, Fail>, backing: bool) -> Result<Value, Fail> {
    self.unwrap_written(result, backing)
  }

  /// `l op r` as an `Arith` reads it, an object that converts itself read through [`Self::primitive`].
  pub(crate) fn arith(&mut self, op: ArithOp, l: Value, r: Value) -> Result<Value, Fail> {
    if converts_itself(&l) || converts_itself(&r) {
      let (l, r) = self.operands(l, r)?;
      return arith(op, l, r);
    }
    arith(op, l, r)
  }

  /// `l op r` as a `Compare` reads it.
  pub(crate) fn compare(&mut self, op: CompareOp, l: Value, r: Value) -> Result<bool, Fail> {
    if !matches!(op, CompareOp::Eq | CompareOp::Ne) && (converts_itself(&l) || converts_itself(&r)) {
      let (l, r) = self.operands(l, r)?;
      return compare(op, &l, &r);
    }
    compare(op, &l, &r)
  }

  /// `MapKey` or `MapOf` on their arguments: an object or an array is kept under its identity, anything else under its text.
  pub(super) fn keyed(&mut self, name: Builtin, values: Vec<Value>) -> Result<Value, Fail> {
    let mut values = values.into_iter();
    let first = values.next().unwrap_or(Value::Undefined);
    if name == Builtin::MapKey {
      return self.map_key_of(first).map(Value::str);
    }
    let pairs = match first {
      Value::Seq(pairs) => pairs,
      Value::Null | Value::Undefined => return Ok(Value::Map(ValueMap::default())),
      other => return Err(type_error("Map", "an array of [key, value] pairs", &other)),
    };
    let mut out = ValueMap::default();
    for pair in pairs.iter() {
      let Value::Seq(kv) = pair else { return Err(type_error("Map", "an array of [key, value] pairs", pair)) };
      let key = self.map_key_of(kv.first().cloned().unwrap_or(Value::Undefined))?;
      out.insert(key, kv.get(1).cloned().unwrap_or(Value::Undefined));
    }
    Ok(Value::Map(out))
  }

  /// `replace` with a function under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn replace_sync(&mut self, subject: &Expr, pattern: &Expr, f: &Expr) -> Result<Value, Fail> {
    let subject = self.eval_sync(subject)?;
    let pattern = self.eval_sync(pattern)?;
    let (s, found) = replacements(&subject, &pattern)?;
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (start, end, args) in found {
      out.push_str(&s[last..start]);
      out.push_str(&stringify(&self.apply_sync(f, args)?)?);
      last = end;
    }
    out.push_str(&s[last..]);
    Ok(Value::str(out))
  }

  /// `toSorted` under `eval_sync`. A function of its own, so the frame `eval_sync` takes at every level of a render stays small.
  #[inline(never)]
  pub(super) fn sort_sync(&mut self, over: &Expr, f: &Expr) -> Result<Value, Fail> {
    let mut items = self.seq_sync(over, "toSorted")?;
    let mut width = 1;
    while width < items.len() {
      let mut merged = Vec::with_capacity(items.len());
      let mut start = 0;
      while start < items.len() {
        let mid = (start + width).min(items.len());
        let end = (start + 2 * width).min(items.len());
        let (mut a, mut b) = (start, mid);
        while a < mid && b < end {
          let order = match f {
            Expr::Lit(Lit::Null) | Expr::Folded(Value::Null) => default_order(&items[a], &items[b])?,
            f => comparator_order(&self.apply_sync(f, vec![items[a].clone(), items[b].clone()])?),
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


  pub(super) fn seq_sync(&mut self, over: &Expr, what: &str) -> Result<Vec<Value>, Fail> {
    match as_items(self.eval_sync(over)?) {
      Value::Seq(items) => Ok(items.into_items()),
      other => Err(type_error(what, "an array", &other)),
    }
  }
}
