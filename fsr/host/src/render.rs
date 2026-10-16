//! The render pipeline: a route or an intercept to its assembled stream, in HTML, payload or fragment mode.

use super::*;

/// An intercepted render: the page the document is showing, as the navigator
/// sent it in `x-sf-from` with its locale prefix stripped, its params and its
/// query, plus the plan nodes the document keeps, which load and are keyed
/// under that request rather than the navigation's.
#[derive(Clone)]
pub(crate) struct Intercept {
  pub(crate) document: String,
  pub(crate) params: Params,
  pub(crate) query: Params,
  pub(crate) kept: Vec<u32>,
}

/// What one render produced: the chunks to stream and, when the route's own
/// page loader failed, the kind, which is what sets the document's status.
pub(crate) struct Rendered {
  pub(crate) chunks: BoxStream<'static, String>,
  pub(crate) failed: Option<FailureKind>,
}

/// The status a document answers with: the page's failure kind when its
/// loader failed, since the entity the route names is what a crawler, a cache
/// and a browser's history read the status for; a layout or a slot failing
/// leaves the page and the status alone.
pub(crate) fn status_of(failed: Option<FailureKind>) -> StatusCode {
  failed.map(|kind| StatusCode::from_u16(kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)).unwrap_or(StatusCode::OK)
}

/// The slot an intercept plan fills: the child of the node that keeps the
/// page which the plan fills instead. A node that keeps other slots but still
/// fills its own `content` is a layout on the way down, not the one declaring
/// the slot, so the walk continues through it.
pub(crate) fn intercept_slot(plan: &PlanNode) -> Option<String> {
  if plan.keep.iter().any(|name| name.0 == "content") {
    return plan
      .children
      .iter()
      .find(|(name, _)| !plan.keep.contains(name))
      .map(|(name, _)| name.0.clone());
  }
  plan.children.iter().find_map(|(_, child)| intercept_slot(child))
}

/// The nodes of the intercept plan's spine the document keeps: from the root
/// down to the layout declaring the slot, as far as each is the same module at
/// the same depth on `from`.
pub(crate) fn kept_spine(intercept: &PlanNode, from: &PlanNode) -> Vec<u32> {
  let mut kept = Vec::new();
  let (mut here, mut there) = (intercept, from);
  loop {
    if here.module != there.module {
      return kept;
    }
    kept.push(here.id.0);
    if here.keep.iter().any(|name| name.0 == "content") {
      return kept;
    }
    let next = here.children.iter().find(|(name, _)| name.0 == "content");
    let from_next = there.children.iter().find(|(name, _)| name.0 == "content");
    match (next, from_next) {
      (Some((_, a)), Some((_, b))) => {
        here = a;
        there = b;
      }
      _ => return kept,
    }
  }
}

/// True when every layout on the intercept plan's spine, down to the one
/// declaring its slot, is the same module at the same depth on `from`.
pub(crate) fn shares_layouts(intercept: &PlanNode, from: &PlanNode) -> bool {
  if intercept.module != from.module {
    return false;
  }
  if intercept.keep.iter().any(|name| name.0 == "content") {
    return true;
  }
  let next = intercept.children.iter().find(|(name, _)| name.0 == "content");
  let from_next = from.children.iter().find(|(name, _)| name.0 == "content");
  match (next, from_next) {
    (Some((_, a)), Some((_, b))) => shares_layouts(a, b),
    _ => false,
  }
}

impl Host {
  /// Renders a route. `path` may carry a locale prefix and its query string.
  /// The session is the caller's, so a test can hand in one it prepared.
  pub async fn render(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_in(&t, &visit, raw_query, mode, Incoming::anonymous(session))
      .await
      .map(|rendered| rendered.chunks)
  }

  pub(crate) async fn render_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    mode: RenderMode,
    incoming: Incoming,
  ) -> Result<Rendered, HostError> {
    let (plan, params) = self
      .plan_for(t, &visit.path)
      .ok_or_else(|| HostError::NotFound(visit.path.clone()))?;
    self
      .render_plan(t, &plan, params, parse_query(raw_query), mode, incoming, visit)
      .await
  }

  /// The plan a route resolves `path` to, with its params.
  pub(crate) fn plan_for(&self, t: &Tables, path: &str) -> Option<(PlanNode, Params)> {
    let span = tracing::info_span!(target: "fsr::trace", "match", pattern = tracing::field::Empty).entered();
    let matched = t.app.matcher.match_path(path)?;
    if let Some(pattern) = t.app.patterns.get(matched.entry.0 as usize) {
      span.record("pattern", pattern.as_str());
    }
    let plan = t.app.resolver.resolve(matched.entry, &matched.params)?;
    Some((plan, matched.params))
  }

  /// The intercept a soft navigation to `path` renders: of the route's
  /// `page.<slot>.tsx` plans, in file order, the first whose slot `into`
  /// names, or, without `into`, the first whose layouts the route of `from`
  /// reaches down to the one declaring its slot. `path` and `from` are paths
  /// without their query.
  pub fn intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)> {
    let t = self.tables();
    self.intercept_in(&t, path, from, into)
  }

  pub(crate) fn intercept_in(&self, t: &Tables, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)> {
    let (plans, params) = t.app.intercepts.plans_for(path)?;
    let from_plan = match into {
      Some(_) => None,
      None => Some(self.plan_for(t, from?)?.0),
    };
    let chosen = plans.into_iter().find(|plan| match (into, &from_plan) {
      (Some(slot), _) => intercept_slot(plan).as_deref() == Some(slot),
      (None, Some(from_plan)) => shares_layouts(plan, from_plan),
      (None, None) => false,
    })?;
    Some((chosen, params))
  }

  /// The payload for a soft navigation to `path` from `from`: the intercept
  /// when one applies, the route's own tree otherwise. `path` may carry a
  /// locale prefix and its query; `from` is the document's `pathname` plus
  /// `search`, prefix included.
  pub async fn render_navigation(
    &self,
    path: &str,
    from: Option<&str>,
    into: Option<&str>,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_navigation_in(&t, &visit, raw_query, from, into, Incoming::anonymous(session))
      .await
      .map(|rendered| rendered.chunks)
  }

  pub(crate) async fn render_navigation_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    from: Option<&str>,
    into: Option<&str>,
    incoming: Incoming,
  ) -> Result<Rendered, HostError> {
    let (from_path, from_query) = match from.map(|f| f.split_once('?').unwrap_or((f, ""))) {
      Some((path, query)) => (Some(path), query),
      None => (None, ""),
    };
    let from_bare = from_path.map(|f| t.locales.resolve(f, None, None).path);
    match self.intercept_in(t, &visit.path, from_bare.as_deref(), into) {
      Some((plan, params)) => {
        let mut incoming = incoming;
        incoming.intercept = from_bare.map(|document| {
          let (kept, params) = self.plan_for(t, &document).map(|(from_plan, params)| (kept_spine(&plan, &from_plan), params)).unwrap_or_default();
          Intercept { document, params, query: parse_query(from_query), kept }
        });
        self
          .render_plan(
            t,
            &plan,
            params,
            parse_query(raw_query),
            RenderMode::Payload,
            incoming,
            visit,
          )
          .await
      }
      None => self.render_in(t, visit, raw_query, RenderMode::Payload, incoming).await,
    }
  }

  /// The application's not-found tree for a path no route matches or `None`
  /// when it has none. `params.path` carries the path the tree is answering,
  /// its locale prefix stripped.
  pub async fn render_not_found(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<Option<BoxStream<'static, String>>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_not_found_in(&t, &visit, raw_query, mode, Incoming::anonymous(session))
      .await
      .map(|rendered| rendered.map(|r| r.chunks))
  }

  pub(crate) async fn render_not_found_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    mode: RenderMode,
    incoming: Incoming,
  ) -> Result<Option<Rendered>, HostError> {
    let Some(plan) = &t.app.not_found else { return Ok(None) };
    let mut params = Params::new();
    params.insert("path".to_owned(), visit.path.clone());
    Ok(Some(
      self
        .render_plan(t, plan, params, parse_query(raw_query), mode, incoming, visit)
        .await?,
    ))
  }

  /// The head a request renders under: the boot head, plus the live-refresh
  /// script when `dev` is on and a canonical link when a prefixed request
  /// asked for the default locale.
  pub(crate) async fn render_plan(
    &self,
    t: &Tables,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    visit: &Resolution,
  ) -> Result<Rendered, HostError> {
    let mut extra = Vec::new();
    if matches!(mode, RenderMode::Html)
      && let Some(token) = self.traces.as_ref().and_then(|traces| traces.current()).and_then(|trace| self.trace_token(t, &trace))
    {
      extra.push(snapfire_fsr_core::Node::raw(shell::request_meta(&token)));
    }
    if let Some(facts) = &t.dev_bundle {
      let bundle = match &self.changed {
        Some(reload) => held_bundle(&reload.state, facts),
        None => bundle_id(facts),
      };
      extra.push(snapfire_fsr_core::Node::raw(shell::dev_script(&bundle, &t.styles, t.dev_nonce.as_deref())));
    }
    if visit.prefixed && visit.locale.is_default {
      extra.push(snapfire_fsr_core::Node::raw(shell::canonical(self.origin.as_deref(), &visit.path)));
    }
    let site = t.site_for(&visit.path);
    if let Some(site) = site {
      for href in &site.preload {
        extra.push(snapfire_fsr_core::Node::raw(shell::preload_link(href)));
      }
    }
    let entry = site.and_then(SiteTables::entry);
    let styles: Vec<String> = site.map(SiteTables::styles).unwrap_or_default();
    let catalog = t.catalogs.json(&visit.locale.tag);
    let mut payload_catalog = None;
    if let Some(json) = &catalog {
      match mode {
        RenderMode::Html => extra.push(snapfire_fsr_core::Node::raw(shell::catalog_script(
          &visit.locale.tag,
          json,
        ))),
        RenderMode::Payload => {
          if incoming.held_catalog.as_deref() != Some(visit.locale.tag.as_str()) {
            payload_catalog = Some(json.to_string());
          }
        }
        RenderMode::Fragment(_) => {}
      }
    }
    let localized = t.head_for(&visit.locale.tag);
    if extra.is_empty() && styles.is_empty() && entry.is_none() && payload_catalog.is_none() && localized.is_none() && incoming.theme.is_none() {
      return self
        .render_plan_with(
          t,
          plan,
          params,
          query,
          mode,
          incoming,
          &visit.requested(),
          &visit.locale,
          &t.head,
        )
        .await;
    }
    let mut head = localized.unwrap_or_else(|| t.head.clone());
    let mut parts = vec![t.head.rest.clone()];
    parts.extend(extra);
    head.rest = snapfire_fsr_core::Node::Seq(parts);
    head.entry = entry;
    head.styles = styles;
    head.catalog = payload_catalog;
    head.theme = incoming.theme.clone();
    self
      .render_plan_with(
        t,
        plan,
        params,
        query,
        mode,
        incoming,
        &visit.requested(),
        &visit.locale,
        &head,
      )
      .await
  }

  pub(crate) async fn render_plan_with(
    &self,
    t: &Tables,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    path: &str,
    locale: &Locale,
    head: &Head,
  ) -> Result<Rendered, HostError> {
    let origin = incoming.intercept.as_ref().filter(|i| !i.kept.is_empty()).map(|i| (i.document.clone(), i.params.clone(), i.query.clone(), i.kept.clone()));
    let ctx = self.ctx(t, incoming, params, query, path, locale.clone());
    let assembly = match origin {
      Some((document, params, query, nodes)) => {
        let mut under = ctx.clone();
        under.path = document;
        under.params = params;
        under.query = query;
        assemble_under(&t.app.runtime, plan, &ctx, head, Origin { ctx: under, nodes }).await?
      }
      None => assemble(&t.app.runtime, plan, &ctx, head).await?,
    };
    let failed = assembly.failed;
    let chunks: BoxStream<'static, String> = match mode {
      RenderMode::Html => Box::pin(html_stream(assembly)),
      RenderMode::Payload => Box::pin(wire_stream(assembly)),
      RenderMode::Fragment(slot) => {
        let html = snapfire_fsr_runtime::fragment_html(assembly, slot.as_deref())
          .await
          .ok_or_else(|| HostError::NoSlot(slot.clone().unwrap_or_default()))?;
        Box::pin(futures_util::stream::once(async move { html }))
      }
    };
    Ok(Rendered { chunks, failed })
  }

  pub async fn render_navigation_to_string(
    &self,
    path: &str,
    from: Option<&str>,
    into: Option<&str>,
    session: SessionCell,
  ) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render_navigation(path, from, into, session).await?.collect().await;
    Ok(chunks.concat())
  }

  /// Renders to one string, for tests.
  pub async fn render_to_string(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render(path, mode, session).await?.collect().await;
    Ok(chunks.concat())
  }

  /// `render_to_string` with the status `handle` would answer: the page
  /// loader's failure kind when it failed, `200` otherwise. `locale` is the
  /// request's `Accept-Language`, which a path prefix outranks.
  pub async fn render_with_status(&self, path: &str, mode: RenderMode, session: SessionCell, locale: Option<&str>) -> Result<(StatusCode, String), HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, locale);
    let rendered = self.render_in(&t, &visit, raw_query, mode, Incoming::anonymous(session)).await?;
    let chunks: Vec<String> = rendered.chunks.collect().await;
    Ok((status_of(rendered.failed), chunks.concat()))
  }

  /// `render_navigation_to_string` with the status `handle` would answer;
  /// `locale` as `render_with_status` takes it.
  pub async fn render_navigation_with_status(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell, locale: Option<&str>) -> Result<(StatusCode, String), HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, locale);
    let rendered = self.render_navigation_in(&t, &visit, raw_query, from, into, Incoming::anonymous(session)).await?;
    let chunks: Vec<String> = rendered.chunks.collect().await;
    Ok((status_of(rendered.failed), chunks.concat()))
  }

  /// Drops every cached subtree of the plan node keyed `plan_key`, a module
  /// name for a lowered page or layout, then says how many went. Zero when
  /// nothing was cached under it or no cache is configured.
  pub async fn invalidate(&self, plan_key: &str) -> usize {
    self.tables().app.invalidate(plan_key).await
  }

  /// The service registry the routes call through, for a Rust host that
  /// calls a backend outside a request.
  pub fn services(&self) -> Arc<Services> {
    self.tables().app.services.clone()
  }

  /// Drops every data cache answer under the named tags, the out-of-band
  /// counterpart of a method that `writes` them.
  pub fn invalidate_tags<I, S>(&self, tags: I)
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    self.tables().app.services.invalidate_tags(tags);
  }
}
