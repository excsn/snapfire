use super::*;

impl Kind {
  pub(crate) fn name(self) -> &'static str {
    match self {
      Kind::Map => "Map",
      Kind::Set => "Set",
      Kind::Query => "URLSearchParams",
      Kind::Url => "URL",
      Kind::Date => "Date",
      Kind::BigInt => "BigInt",
      Kind::Response => "Response",
      Kind::Browser => "value only the browser knows",
      Kind::Scope => "Scope",
      Kind::Temporal => "Temporal",
      Kind::WeakMap => "WeakMap",
      Kind::WeakSet => "WeakSet",
      Kind::WeakRef => "WeakRef",
      Kind::Registry => "FinalizationRegistry",
    }
  }

  /// The collection a kind reads and writes as: a `WeakMap` as a `Map` and a `WeakSet` as a `Set`.
  pub(crate) fn collection(self) -> Kind {
    match self {
      Kind::WeakMap => Kind::Map,
      Kind::WeakSet => Kind::Set,
      other => other,
    }
  }

  /// Whether the kind holds its keys weakly, so a key must be an object.
  pub(crate) fn weak(self) -> bool {
    matches!(self, Kind::WeakMap | Kind::WeakSet)
  }

  /// `built` marked as the collection it makes, for a `Map` or a `Set`, so it converts and serializes as one wherever it goes.
  pub(crate) fn made(self, built: Expr) -> Expr {
    match self {
      Kind::Map | Kind::WeakMap => Expr::Builtin { name: Builtin::AsMap, args: vec![built] },
      Kind::Set | Kind::WeakSet => Expr::Builtin { name: Builtin::AsSet, args: vec![built] },
      _ => built,
    }
  }

  /// The variant tag the value travels under into an island, so the browser makes the real type; a `URLSearchParams` is a string on both sides.
  pub(crate) fn tag(self) -> Option<&'static str> {
    use snapfire_fsr_ir::render::typed;
    match self {
      Kind::Map => Some(typed::MAP),
      Kind::Set => Some(typed::SET),
      Kind::Query | Kind::Browser | Kind::Response | Kind::Scope | Kind::WeakMap | Kind::WeakSet | Kind::WeakRef | Kind::Registry => None,
      Kind::Url => Some(typed::URL),
      Kind::Date => Some(typed::DATE),
      Kind::BigInt => Some(typed::BIGINT),
      Kind::Temporal => Some(typed::TEMPORAL),
    }
  }

  /// The kind a type annotation names: `Map<K, V>`, `Set<T>`, `Date`, `URLSearchParams`, `URL` or `bigint`.
  pub(crate) fn of_type(ty: &js::TSType) -> Option<Kind> {
    Kind::of_type_in(ty, &[])
  }

  /// The kind `ty` names, an alias in `aliases` read through to what it stands for, so `Unique<URL>` for `type Unique<T> = T & Brand` is a `URL`.
  pub(crate) fn of_type_in(ty: &js::TSType, aliases: &[(String, AliasKind)]) -> Option<Kind> {
    match ty {
      js::TSType::TSParenthesizedType(p) => Kind::of_type_in(&p.type_annotation, aliases),
      js::TSType::TSIntersectionType(i) => i.types.iter().find_map(|t| Kind::of_type_in(t, aliases)),
      js::TSType::TSBigIntKeyword(_) => Some(Kind::BigInt),
      js::TSType::TSTypeReference(r) => match &r.type_name {
        js::TSTypeName::QualifiedName(q) if matches!(&q.left, js::TSTypeName::IdentifierReference(id) if id.name.as_str() == "Temporal") => Some(Kind::Temporal),
        js::TSTypeName::IdentifierReference(id) => match id.name.as_str() {
          "Map" | "ReadonlyMap" => Some(Kind::Map),
          "Set" | "ReadonlySet" => Some(Kind::Set),
          "Date" => Some(Kind::Date),
          "URLSearchParams" => Some(Kind::Query),
          "URL" => Some(Kind::Url),
          name => match aliases.iter().find(|(n, _)| n == name).map(|(_, kind)| *kind)? {
            AliasKind::Fixed(kind) => Some(kind),
            AliasKind::Param(i) => Kind::of_type_in(r.type_arguments.as_ref()?.params.get(i)?, aliases),
          },
        },
        _ => None,
      },
      _ => None,
    }
  }
}
