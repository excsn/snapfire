use super::*;

impl Env {
  /// An environment for a render: no request, no session, no input; only
  /// the scope a component's props make.
  pub(crate) fn detached(interpreter: &Interpreter, scope: Vec<(Name, Value)>) -> Self {
    Self {
      store_keys: interpreter.store_keys.clone(),
      ctx: RequestCtx::anonymous(Default::default()),
      input: Value::Null,
      identity: None,
      session: ValueMap::default(),
      written: Vec::new(),
      extend: None,
      scope,
      store: ValueMap::default(),
      pending: Default::default(),
      clock: interpreter.clock.clone(),
      extensions: interpreter.extensions.clone(),
      catalogs: interpreter.catalogs.clone(),
      files: interpreter.files.clone(),
      consts: interpreter.consts.clone(),
      functions: interpreter.functions.clone(),
      held: interpreter.held.clone(),
      hoists: None,
      state: None,
      probe: None,
      entered: Vec::new(),
      emitted: Vec::new(),
      server_mode: false,
      acts: Vec::new(),
      handling: false,
      calls: 0,
      depth: 0,
      frameworks: interpreter.frameworks,
      island_reads: interpreter.island_reads.clone(),
      derives: interpreter.derives.clone(),
      markup: Markup::Plain,
      in_svg: false,
      in_noscript: false,
      in_framework: false,
      component_path: String::new(),
      unrendered: false,
      contexts: Vec::new(),
      island: None,
      stamp: Vec::new(),
      fallthrough: None,
      slotted: Vec::new(),
      slot_stamps: None,
      scoped: None,
      made: Some(Vec::new()),
      closures: Some(Vec::new()),
      cells: Some(Vec::new()),
      heap_live: false,
      heap_copies: Vec::new(),
      thawed: Default::default(),
      object_keys: Vec::new(),
      module_cells: Vec::new(),
      store_written: ValueMap::default(),
      handler_calls: Vec::new(),
      written_back: None,
      backing: false,
      module_seed: snapfire_fsr_core::ValueMap::default(),
      cycles: Cycles::default(),
      marks: Vec::new(),
      placed: Vec::new(),
    }
  }

  pub(super) fn lookup(&self, name: &str) -> Result<Value, Fail> {
    self
      .scope
      .iter()
      .rev()
      .find(|(n, _)| n == name)
      .map(|(_, v)| v.clone())
      .ok_or_else(|| Fail::internal(format!("`{name}` is not bound")))
  }

  /// Whether `expr` is truthy, read through its place when it has one.
  pub(crate) fn truth_of(&mut self, expr: &Expr) -> Result<bool, Fail> {
    match self.place(expr) {
      Some(value) => Ok(truthy(value)),
      None => Ok(truthy(&self.eval_sync(expr)?)),
    }
  }

  /// `None` is always safe: it means the caller must evaluate the expression,
  /// never that the expression has no value.
  pub(crate) fn place<'a>(&'a self, expr: &'a Expr) -> Option<&'a Value> {
    if self.heap_live {
      return self.place_live(expr);
    }
    match expr {
      Expr::Var(name) => self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v),
      Expr::Folded(value) => Some(value),
      Expr::Store(key) => self.store.get(key),
      Expr::Session(key) => self.session.get(key),
      Expr::Const(key) => self.held.get(key),
      Expr::Field(target, name) => match self.place(target)? {
        Value::Map(map) => map.get(name.as_str()),
        _ => None,
      },
      Expr::Index(target, key) => match (self.place(target)?, &**key) {
        (Value::Map(map), Expr::Lit(Lit::Str(k))) => map.get(k.as_str()),
        (Value::Map(map), Expr::Folded(Value::Str(k))) => map.get(k.as_str()),
        (Value::Seq(items), Expr::Lit(Lit::Int(i))) => usize::try_from(*i).ok().and_then(|i| items.get(i)),
        (Value::Seq(items), Expr::Folded(Value::Int(i))) => usize::try_from(*i).ok().and_then(|i| items.get(i)),
        _ => None,
      },
      _ => None,
    }
  }

  /// The request's locale or null under a context that has none.
  /// What the request, the session draft or the clock answer for `read`.
  pub(crate) fn read_request(&self, read: Read<'_>) -> Value {
    match read {
      Read::Param(name) => self.ctx.params.get(name).map(|s| Value::str(s.clone())).unwrap_or(Value::Undefined),
      Read::Query(name) => self.ctx.query.get(name).map(|s| Value::str(s.clone())).unwrap_or(Value::Undefined),
      Read::ParamsAll => Value::Map(self.ctx.params.iter().map(|(k, v)| (k.clone(), Value::str(v.clone()))).collect()),
      Read::QueryAll => Value::Map(self.ctx.query.iter().map(|(k, v)| (k.clone(), Value::str(v.clone()))).collect()),
      Read::Session(key) => self.session.get(key).cloned().unwrap_or(Value::Null),
      Read::Locale => self.locale(),
      Read::Locales => Value::seq(self.ctx.locales.iter().map(|tag| Value::str(tag.clone())).collect::<Vec<_>>()),
      Read::Path => Value::str(self.ctx.path.clone()),
      Read::Document => Value::str(self.ctx.document.clone().unwrap_or_else(|| self.ctx.path.clone())),
      Read::Host => self.ctx.host.clone().map(Value::str).unwrap_or(Value::Null),
      Read::Origin => self.ctx.origin.clone().map(Value::str).unwrap_or(Value::Null),
      Read::Address => self.ctx.address.as_ref().map(snapfire_fsr_runtime::Address::value).unwrap_or(Value::Null),
      Read::Config(key) => self.ctx.config.get(key).cloned().unwrap_or(Value::Null),
      Read::Input => self.input.clone(),
      Read::Now => Value::Int(self.clock.now()),
    }
  }

  pub(super) fn locale(&self) -> Value {
    if self.ctx.locale.tag.is_empty() {
      Value::Null
    } else {
      Value::str(self.ctx.locale.tag.clone())
    }
  }

  pub(super) fn touch(&mut self, key: &str) {
    self.forget_copy('s', key);
    if !self.written.iter().any(|k| k == key) {
      self.written.push(key.to_owned());
    }
  }

  /// The store key `key` set to `value` or removed for `None`. A copy [`Self::own_read`] kept of it is dropped, so the next read sees the write.
  pub(crate) fn set_store(&mut self, key: &str, value: Option<Value>) {
    self.forget_copy('t', key);
    match value {
      Some(value) => {
        self.store.insert(key, value);
      }
      None => {
        self.store.shift_remove(key);
      }
    }
  }

  pub(super) fn forget_copy(&mut self, tag: char, key: &str) {
    if self.heap_copies.is_empty() {
      return;
    }
    self.heap_copies.retain(|(held, _)| !(held.starts_with(tag) && held.get(1..2) == Some(":") && &held[2..] == key));
  }

  pub(super) async fn guard(&mut self, cond: &Expr, kind: &str, message: &Expr) -> Result<(), Fail> {
    if truthy(&self.eval(cond).await?) {
      let kind = parse_kind(kind).ok_or_else(|| Fail::internal(format!("`{kind}` is not a failure kind")))?;
      let message = stringify(&self.eval(message).await?)?;
      return Err(Fail::new(kind, message));
    }
    Ok(())
  }

  /// `block` for a body with no service call in it: the statements a `Block` expression or a helper's body holds. A statement that reaches a service or the session is the async path's, so it fails here.
  /// `name = { ...name, k: v }`, `name = [...name, v]` or `name = setAt(name, k, v)` changing the local's own object. It stays a copy when anything else holds that object, since `make_mut` copies a shared one.
  pub(super) fn set_in_place(&mut self, name: &str, expr: &Expr) -> Result<bool, Fail> {
    enum Write {
      Fields(Vec<(String, Value)>),
      Items(Vec<Value>),
      At(Value, Value),
    }
    let own = |e: &Expr| matches!(e, Expr::Var(n) if n == name);
    let write = match expr {
      Expr::Object(entries) => {
        let Some((Entry::Spread(first), rest)) = entries.split_first() else { return Ok(false) };
        if !own(first) || rest.is_empty() || !rest.iter().all(|e| matches!(e, Entry::Field(..) | Entry::Computed(..))) {
          return Ok(false);
        }
        if !matches!(self.scope.iter().rev().find(|(n, _)| n == name), Some((_, Value::Map(_)))) {
          return Ok(false);
        }
        let mut out = Vec::with_capacity(rest.len());
        for entry in rest {
          match entry {
            Entry::Field(key, value) => out.push((key.clone(), self.eval_sync(value)?)),
            Entry::Computed(key, value) => {
              let key = key_text(&self.eval_sync(key)?)?;
              out.push((key, self.eval_sync(value)?));
            }
            _ => unreachable!("checked"),
          }
        }
        Write::Fields(out)
      }
      Expr::Array(entries) => {
        let Some((Entry::Spread(first), rest)) = entries.split_first() else { return Ok(false) };
        if !own(first) || rest.is_empty() || !rest.iter().all(|e| matches!(e, Entry::Item(_))) {
          return Ok(false);
        }
        if !matches!(self.scope.iter().rev().find(|(n, _)| n == name), Some((_, Value::Seq(_)))) {
          return Ok(false);
        }
        let mut out = Vec::with_capacity(rest.len());
        for entry in rest {
          let Entry::Item(value) = entry else { unreachable!("checked") };
          out.push(self.eval_sync(value)?);
        }
        Write::Items(out)
      }
      Expr::Builtin { name: Builtin::SetAt, args } if args.len() == 3 && own(&args[0]) => Write::At(self.eval_sync(&args[1])?, self.eval_sync(&args[2])?),
      _ => return Ok(false),
    };
    let Some(slot) = self.scope.iter_mut().rev().find(|(n, _)| n == name) else { return Ok(false) };
    match (write, &mut slot.1) {
      (Write::Fields(fields), Value::Map(map)) => {
        for (key, value) in fields {
          map.insert(key, value);
        }
      }
      (Write::Items(items), Value::Seq(seq)) => seq.extend(items),
      (Write::At(key, value), Value::Map(map)) => {
        map.insert(stringify(&key)?, value);
      }
      (Write::At(key, value), target @ Value::Seq(_)) => {
        let held = std::mem::replace(target, Value::Null);
        *target = builtin(Builtin::SetAt, vec![held, key, value])?;
      }
      (Write::At(key, value), target) => {
        let held = target.clone();
        *target = builtin(Builtin::SetAt, vec![held, key, value])?;
      }
      _ => return Ok(false),
    }
    Ok(true)
  }

  pub(crate) fn block_sync(&mut self, body: &Body) -> Result<Flow, Fail> {
    let depth = self.scope.len();
    for stmt in body {
      match stmt {
        Stmt::Let { name, expr } => {
          let value = self.eval_sync(expr)?;
          self.scope.push((Name::new(name), value));
        }
        Stmt::Set { name, expr } => {
          if self.set_in_place(name, expr)? {
            continue;
          }
          let value = self.eval_sync(expr)?;
          let Some(slot) = self.scope.iter_mut().rev().find(|(n, _)| n == name) else {
            return Err(Fail::internal(format!("`{name}` is set before it is bound")));
          };
          slot.1 = value;
        }
        Stmt::If { cond, then, r#else } => {
          let branch = if truthy(&self.eval_sync(cond)?) { then } else { r#else };
          match self.block_sync(branch)? {
            Flow::Next => {}
            flow => {
              self.scope.truncate(depth);
              return Ok(flow);
            }
          }
        }
        Stmt::ForOf { name, over, body } => {
          let items = match self.eval_sync(over)? {
            Value::Seq(items) => items,
            other => return Err(type_error("for...of", "an array", &other)),
          };
          for item in items {
            self.scope.push((Name::new(name), item));
            let flow = self.block_sync(body);
            self.scope.pop();
            match flow? {
              Flow::Next | Flow::Continue => {}
              Flow::Break => break,
              Flow::Return(v) => {
                self.scope.truncate(depth);
                return Ok(Flow::Return(v));
              }
            }
          }
        }
        Stmt::While { cond, body, step } => {
          let mut passes = 0;
          while truthy(&self.eval_sync(cond)?) {
            passes += 1;
            if passes > MAX_PASSES {
              return Err(past_passes());
            }
            match self.block_sync(body)? {
              Flow::Next | Flow::Continue => {}
              Flow::Break => break,
              Flow::Return(v) => {
                self.scope.truncate(depth);
                return Ok(Flow::Return(v));
              }
            }
            self.block_sync(step)?;
          }
        }
        Stmt::Return(expr) => {
          let value = self.eval_sync(expr)?;
          self.scope.truncate(depth);
          return Ok(Flow::Return(value));
        }
        Stmt::Break => {
          self.scope.truncate(depth);
          return Ok(Flow::Break);
        }
        Stmt::Continue => {
          self.scope.truncate(depth);
          return Ok(Flow::Continue);
        }
        Stmt::Try { body, name, handler, finally } => match self.try_sync(body, name.as_deref(), handler.as_ref(), finally)? {
          Flow::Next => {}
          flow => {
            self.scope.truncate(depth);
            return Ok(flow);
          }
        },
        Stmt::Expr(expr) => {
          self.eval_sync(expr)?;
        }
        Stmt::Act { action, input } if self.server_mode && !self.handling => {
          let input = self.eval_sync(input)?;
          let mut act = ValueMap::default();
          act.insert("action".to_owned(), Value::str(action));
          act.insert("input".to_owned(), input);
          self.emitted.push(crate::render::Emitted { path: self.component_path.clone(), kind: crate::render::ACT.to_owned(), value: Value::Map(act) });
        }
        Stmt::Act { action, input } if self.handling => {
          let input = self.eval_sync(input)?;
          self.acts.push((action.clone(), input));
        }
        Stmt::Guard { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } | Stmt::Act { .. } => return Err(Fail::internal("a guard, a session write, a redirect or a dispatch inside a block, which runs where no request is")),
      }
    }
    self.scope.truncate(depth);
    Ok(Flow::Next)
  }

  /// A `Try` under `block_sync`, in a frame of its own so the frame `block_sync` takes for every statement stays small.
  #[inline(never)]
  pub(super) fn try_sync(&mut self, body: &Body, name: Option<&str>, handler: Option<&Body>, finally: &Body) -> Result<Flow, Fail> {
    let at = self.scope.len();
    let mut flow = self.block_sync(body);
    self.scope.truncate(at);
    if let (Some(handler), Err(fail)) = (handler, &flow) {
      if let Some(caught) = caught(fail) {
        self.scope.extend(name.map(|name| (Name::new(name), caught)));
        flow = self.block_sync(handler);
        self.scope.truncate(at);
      }
    }
    if !finally.is_empty() {
      let after = self.block_sync(finally);
      self.scope.truncate(at);
      flow = after_finally(flow, after);
    }
    flow
  }

  pub(crate) fn block<'a>(&'a mut self, body: &'a Body) -> BoxFuture<'a, Result<Flow, Fail>> {
    Box::pin(async move {
      let depth = self.scope.len();
      let mut i = 0;
      while i < body.len() {
        let mut run = independent_lets(&body[i..]);
        if run > 1 && self.heap_live && !body[i..i + run].iter().all(|stmt| matches!(stmt, Stmt::Let { expr, .. } if writes_nothing(expr))) {
          run = 1;
        }
        if run > 1 {
          let names_and_values = self.parallel_lets(&body[i..i + run]).await?;
          self.scope.extend(names_and_values.into_iter().map(|(name, value)| (Name::new(name), value)));
          i += run;
          continue;
        }
        match self.stmt(&body[i]).await? {
          Flow::Next => {}
          flow => {
            self.scope.truncate(depth);
            return Ok(flow);
          }
        }
        i += 1;
      }
      self.scope.truncate(depth);
      Ok(Flow::Next)
    })
  }

  pub(super) async fn parallel_lets(&mut self, stmts: &[Stmt]) -> Result<Vec<(String, Value)>, Fail> {
    let futures: Vec<_> = stmts
      .iter()
      .map(|stmt| match stmt {
        Stmt::Let { name, expr } => {
          let name = name.clone();
          let mut snapshot = self.snapshot();
          async move { snapshot.eval(expr).await.map(|v| (name, v)) }
        }
        _ => unreachable!("independent_lets only groups lets"),
      })
      .collect();
    join_all(futures).await.into_iter().collect()
  }

  /// A read-only copy for evaluating in parallel. Session writes are
  /// statements, never expressions, so a copy cannot diverge from the draft.
  pub(super) fn snapshot(&self) -> Env {
    Env {
      store_keys: self.store_keys.clone(),
      ctx: self.ctx.clone(),
      input: self.input.clone(),
      identity: self.identity.clone(),
      session: self.session.clone(),
      written: Vec::new(),
      extend: None,
      scope: self.scope.clone(),
      store: self.store.clone(),
      pending: self.pending.clone(),
      clock: self.clock.clone(),
      extensions: self.extensions.clone(),
      catalogs: self.catalogs.clone(),
      files: self.files.clone(),
      consts: self.consts.clone(),
      functions: self.functions.clone(),
      held: self.held.clone(),
      hoists: None,
      state: None,
      probe: None,
      entered: Vec::new(),
      emitted: Vec::new(),
      server_mode: false,
      acts: Vec::new(),
      handling: false,
      calls: self.calls,
      depth: self.depth,
      frameworks: self.frameworks,
      island_reads: self.island_reads.clone(),
      derives: self.derives.clone(),
      markup: self.markup,
      in_svg: self.in_svg,
      in_noscript: self.in_noscript,
      in_framework: self.in_framework,
      component_path: self.component_path.clone(),
      unrendered: false,
      contexts: self.contexts.clone(),
      island: self.island,
      stamp: Vec::new(),
      fallthrough: None,
      slotted: Vec::new(),
      slot_stamps: None,
      scoped: None,
      made: None,
      closures: None,
      cells: None,
      heap_live: self.heap_live,
      heap_copies: self.heap_copies.clone(),
      thawed: self.thawed.clone(),
      object_keys: self.object_keys.clone(),
      module_cells: Vec::new(),
      store_written: ValueMap::default(),
      handler_calls: Vec::new(),
      written_back: None,
      backing: false,
      module_seed: snapfire_fsr_core::ValueMap::default(),
      cycles: Cycles::default(),
      marks: Vec::new(),
      placed: Vec::new(),
    }
  }

  pub(super) fn ambient(&self) -> Ambient {
    Ambient::default().locale(self.ctx.locale.tag.clone()).now(self.clock.now()).catalogs(self.catalogs.clone()).files(self.files.clone())
  }

  pub(super) async fn stmt(&mut self, stmt: &Stmt) -> Result<Flow, Fail> {
    match stmt {
      Stmt::Let { name, expr } => {
        let value = self.eval(expr).await?;
        self.scope.push((Name::new(name), value));
      }
      Stmt::Set { name, expr } if !expr.has_call() && self.set_in_place(name, expr)? => {}
      Stmt::Set { name, expr } => {
        let value = self.eval(expr).await?;
        let Some(slot) = self.scope.iter_mut().rev().find(|(n, _)| n == name) else {
          return Err(Fail::internal(format!("`{name}` is set before it is bound")));
        };
        slot.1 = value;
      }
      Stmt::If { cond, then, r#else } => {
        let branch = if truthy(&self.eval(cond).await?) { then } else { r#else };
        match self.block(branch).await? {
          Flow::Next => {}
          flow => return Ok(flow),
        }
      }
      Stmt::ForOf { name, over, body } => {
        let items = match self.eval(over).await? {
          Value::Seq(items) => items,
          other => return Err(type_error("for...of", "an array", &other)),
        };
        for item in items {
          self.scope.push((Name::new(name), item));
          let flow = self.block(body).await;
          self.scope.pop();
          match flow? {
            Flow::Next | Flow::Continue => {}
            Flow::Break => break,
            Flow::Return(v) => return Ok(Flow::Return(v)),
          }
        }
      }
      Stmt::While { cond, body, step } => {
        let mut passes = 0;
        while truthy(&self.eval(cond).await?) {
          passes += 1;
          if passes > MAX_PASSES {
            return Err(past_passes());
          }
          match self.block(body).await? {
            Flow::Next | Flow::Continue => {}
            Flow::Break => break,
            Flow::Return(v) => return Ok(Flow::Return(v)),
          }
          self.block(step).await?;
        }
      }
      Stmt::Return(expr) => return Ok(Flow::Return(self.eval(expr).await?)),
      Stmt::Break => return Ok(Flow::Break),
      Stmt::Continue => return Ok(Flow::Continue),
      Stmt::Try { body, name, handler, finally } => {
        let at = self.scope.len();
        let mut flow = self.block(body).await;
        self.scope.truncate(at);
        if let (Some(handler), Err(fail)) = (handler, &flow) {
          if let Some(caught) = caught(fail) {
            self.scope.extend(name.iter().map(|name| (Name::new(name), caught.clone())));
            flow = self.block(handler).await;
            self.scope.truncate(at);
          }
        }
        if !finally.is_empty() {
          let after = self.block(finally).await;
          self.scope.truncate(at);
          flow = after_finally(flow, after);
        }
        match flow? {
          Flow::Next => {}
          flow => return Ok(flow),
        }
      }
      Stmt::Guard { cond, kind, message } => self.guard(cond, kind, message).await?,
      Stmt::SessionSet { key, path, value } => {
        let value = self.eval(value).await?;
        let mut steps = Vec::with_capacity(path.len());
        for step in path {
          steps.push(self.eval(step).await?);
        }
        let root = self.session.entry(key.clone()).or_insert(Value::Null);
        set_path(root, &steps, value)?;
        self.touch(key);
      }
      Stmt::SessionDelete { key, path } => {
        let mut steps = Vec::with_capacity(path.len());
        for step in path {
          steps.push(self.eval(step).await?);
        }
        if steps.is_empty() {
          self.session.shift_remove(key);
        } else if let Some(root) = self.session.get_mut(key) {
          delete_path(root, &steps)?;
        }
        self.touch(key);
      }
      Stmt::SessionExtend { seconds } => {
        let seconds = match self.eval(seconds).await? {
          Value::Int(n) if n > 0 => n as u64,
          Value::F64(f) if f.fract() == 0.0 && f > 0.0 && f <= SAFE_INTEGER => f as u64,
          other => return Err(type_error("session extend", "a positive whole number of seconds", &other)),
        };
        self.extend = Some(seconds);
      }
      Stmt::Act { action, .. } => return Err(Fail::internal(format!("a body cannot dispatch `{action}`; an action is dispatched from a handler"))),
      Stmt::Expr(expr) => {
        self.eval(expr).await?;
      }
    }
    Ok(Flow::Next)
  }

  pub(super) fn hoist_id(&self, id: u32) -> Result<Value, Fail> {
    match &self.hoists {
      Some(hoists) => Ok(Value::str(hoists.id(id))),
      None => Err(Fail::internal("`useId` reached outside a keyed render")),
    }
  }

  /// `eval` for an expression with no service call in it: a component body,
  /// a render, a test's assertion. One plain recursion and no future per node.
  /// `place` for a body that writes objects: only what is its own, so not what requests share.
  pub(super) fn place_live<'a>(&'a self, expr: &'a Expr) -> Option<&'a Value> {
    match expr {
      Expr::Var(name) => self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v).filter(|v| self.thawed.is_empty() || !is_frozen(v)),
      Expr::Field(target, name) => match self.place_live(target)? {
        Value::Map(map) => map.get(name.as_str()).filter(|v| self.thawed.is_empty() || !is_frozen(v)),
        _ => None,
      },
      _ => None,
    }
  }

  /// `value` as this body sees it: the copy it wrote, when it wrote this frozen object.
  #[inline]
  pub(super) fn resolved(&self, value: Value) -> Value {
    if self.thawed.is_empty() {
      return value;
    }
    let addr = match &value {
      Value::Map(map) if map.is_frozen() => map.addr(),
      Value::Seq(items) if items.is_frozen() => items.addr(),
      _ => return value,
    };
    self.thawed.get(&addr).cloned().unwrap_or(value)
  }

  /// A frozen object about to be written, as a copy of its own that this body sees from now on. The whole constant it is part of is copied with it, each object once, so every name that reaches a part of it reaches the same copy.
  pub(crate) fn thawed_for_write(&mut self, value: Value) -> Value {
    if !is_frozen(&value) {
      return value;
    }
    let addr = value.address().expect("frozen is an object or an array");
    if let Some(copy) = self.thawed.get(&addr) {
      return copy.clone();
    }
    let root = value.frozen_root().unwrap_or_else(|| value.clone());
    self.thaw(&root);
    self.thawed.get(&addr).cloned().unwrap_or(value)
  }

  /// Notes `target` when `stored`, about to be written into it, reaches it: the write closes a cycle that counting alone never frees.
  pub(super) fn note_link(&mut self, target: &Value, stored: &Value) {
    if !matches!(stored, Value::Map(_) | Value::Seq(_)) {
      return;
    }
    let Some(at) = target.address() else { return };
    if snapfire_fsr_core::reaches(stored, at, CYCLE_SEARCH) != Some(false) && !self.cycles.0.iter().any(|held| held.address() == Some(at)) {
      self.cycles.0.push(target.clone());
    }
  }

  pub(super) fn thaw(&mut self, value: &Value) -> Value {
    if !is_frozen(value) {
      return value.clone();
    }
    let addr = value.address().expect("frozen is an object or an array");
    if let Some(copy) = self.thawed.get(&addr) {
      return copy.clone();
    }
    let copy = on_enough_stack(|| match value {
      Value::Map(map) => {
        let mut fields = snapfire_fsr_core::Fields::with_capacity(map.len());
        for (k, v) in map.iter() {
          fields.insert(k.clone(), self.thaw(v));
        }
        Value::Map(ValueMap::from(fields).with_kind(map.kind()))
      }
      Value::Seq(items) => Value::Seq(snapfire_fsr_core::ValueSeq::from(items.iter().map(|v| self.thaw(v)).collect::<Vec<_>>()).with_kind(items.kind())),
      other => other.clone(),
    });
    self.thawed.insert(addr, copy.clone());
    copy
  }

  /// `value` with every frozen object this body wrote replaced by its copy, for what leaves the body.
  pub(super) fn settled(&self, value: Value) -> Value {
    if self.thawed.is_empty() {
      return value;
    }
    match self.resolved(value) {
      Value::Map(map) => {
        let mut out = snapfire_fsr_core::Fields::with_capacity(map.len());
        for (k, v) in map.iter() {
          out.insert(k.clone(), self.settled(v.clone()));
        }
        Value::Map(ValueMap::from(out).with_kind(map.kind()))
      }
      Value::Seq(items) => Value::Seq(snapfire_fsr_core::ValueSeq::from(items.iter().map(|v| self.settled(v.clone())).collect::<Vec<_>>()).with_kind(items.kind())),
      other => other,
    }
  }

  /// What a body that writes objects reads from outside itself: its own copy, the same one each time.
  #[inline(never)]
  pub(super) fn own_read(&mut self, expr: &Expr) -> Result<Option<Value>, Fail> {
    let key = match expr {
      Expr::Session(key) => format!("s:{key}"),
      Expr::Store(key) => format!("t:{key}"),
      Expr::Config(key) => format!("f:{key}"),
      Expr::Context(key) => format!("x:{key}"),
      Expr::Input => "i".to_owned(),
      Expr::Identity(path) => format!("d:{path:?}"),
      _ => return Ok(None),
    };
    if let Some((_, value)) = self.heap_copies.iter().find(|(k, _)| *k == key) {
      return Ok(Some(value.clone()));
    }
    self.heap_live = false;
    let value = self.eval_sync(expr);
    self.heap_live = true;
    let value = own_copy(&value?);
    self.heap_copies.push((key, value.clone()));
    Ok(Some(value))
  }

  pub(super) fn overwrite(&mut self, target: &Expr, next: &Expr) -> Result<Value, Fail> {
    let held = self.eval_sync(target)?;
    let held = self.thawed_for_write(held);
    let write = match (&held, next) {
      (Value::Map(_), Expr::Object(entries)) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) && entries[1..].iter().all(|e| matches!(e, Entry::Field(..) | Entry::Computed(..))) => {
        let mut fields = Vec::with_capacity(entries.len() - 1);
        for entry in &entries[1..] {
          fields.push(match entry {
            Entry::Field(key, value) => (Name::new(key), self.eval_sync(value)?),
            Entry::Computed(key, value) => {
              let key = self.eval_sync(key)?;
              (key_name(key)?, self.eval_sync(value)?)
            }
            _ => unreachable!("checked"),
          });
        }
        Write::Fields(fields)
      }
      (Value::Seq(_), Expr::Array(entries)) if matches!(entries.first(), Some(Entry::Spread(first)) if first == target) && entries[1..].iter().all(|e| matches!(e, Entry::Item(_))) => {
        let mut items = Vec::with_capacity(entries.len() - 1);
        for entry in &entries[1..] {
          let Entry::Item(value) = entry else { unreachable!("checked") };
          items.push(self.eval_sync(value)?);
        }
        Write::Items(items)
      }
      (_, Expr::Builtin { name: Builtin::SetAt, args }) if args.len() == 3 && args[0] == *target => {
        let key = self.eval_sync(&args[1])?;
        Write::At(key, self.eval_sync(&args[2])?)
      }
      _ => Write::Whole(self.eval_sync(next)?),
    };
    self.write_into(held, write)
  }

  /// `write`, its parts already evaluated, made in place to `held`, an object or an array this body may write: its own copy of a frozen one, [`Self::thawed_for_write`].
  pub(crate) fn write_into(&mut self, held: Value, write: Write) -> Result<Value, Fail> {
    let frozen = || Fail::internal("a write to an object every request shares");
    match (&held, write) {
      (Value::Map(map), Write::Field(key, value)) => {
        if !map.get(key.as_str()).is_some_and(|old| same_object(old, &value)) {
          self.note_link(&held, &value);
        }
        map.through().ok_or_else(frozen)?.insert(key, value);
      }
      (Value::Seq(items), Write::Item(value)) => {
        self.note_link(&held, &value);
        items.through().ok_or_else(frozen)?.push(value);
      }
      (Value::Map(map), Write::Fields(fields)) => {
        for (key, value) in fields {
          if !map.get(key.as_str()).is_some_and(|old| same_object(old, &value)) {
            self.note_link(&held, &value);
          }
          map.through().ok_or_else(frozen)?.insert(key, value);
        }
      }
      (Value::Seq(items), Write::Items(values)) => {
        for value in values {
          self.note_link(&held, &value);
          items.through().ok_or_else(frozen)?.push(value);
        }
      }
      (Value::Map(map), Write::At(key, value)) => {
        let key = self.map_key_of(key)?;
        if !map.get(key.as_str()).is_some_and(|old| same_object(old, &value)) {
          self.note_link(&held, &value);
        }
        map.through().ok_or_else(frozen)?.insert(key, value);
      }
      (Value::Seq(items), Write::At(key, value)) => {
        let Some(at) = to_number(&key).filter(|at| at.fract() == 0.0 && *at >= 0.0) else { return Err(Fail::internal("a write at a key of an array that is not an index")) };
        self.note_link(&held, &value);
        let items = items.through().ok_or_else(frozen)?;
        let at = at as usize;
        while items.len() <= at {
          items.push(Value::Undefined);
        }
        items[at] = value;
      }
      (_, Write::Whole(next)) => {
        self.note_link(&held, &next);
        match (&held, next) {
          (Value::Map(map), Value::Map(next)) => *map.through().ok_or_else(frozen)? = next.into_fields(),
          (Value::Seq(items), Value::Seq(next)) => *items.through().ok_or_else(frozen)? = next.into_items(),
          (other, _) => return Err(type_error("a write", "an object or an array", other)),
        }
      }
      (other, _) => return Err(type_error("a write", "an object or an array", other)),
    }
    Ok(held)
  }


  /// The text a `Map` or an object keeps `key` under: an object or an array by its identity.
  pub(super) fn map_key_of(&mut self, key: Value) -> Result<String, Fail> {
    let addr = match &key {
      Value::Map(map) => map.addr(),
      Value::Seq(items) => items.addr(),
      other => return stringify(other),
    };
    let text = format!("\u{0}obj:{addr}");
    if !self.object_keys.iter().any(|(k, _)| *k == text) {
      self.object_keys.push((text.clone(), key));
    }
    Ok(text)
  }

  pub(super) fn key_value(&self, key: String) -> Value {
    if key.starts_with('\u{0}') {
      if let Some((_, value)) = self.object_keys.iter().find(|(k, _)| *k == key) {
        return value.clone();
      }
    }
    Value::str(key)
  }
}
