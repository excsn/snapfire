use crate::*;

impl<'a> Lowerer<'a> {
  /// Leaves an `Intl.Segmenter` on a web render path to the browser, which splits by its own Unicode tables. A body and a terminal page split by ICU's.
  pub(crate) fn grapheme_segmenter(&mut self, made: &js::NewExpression) -> Lowered<()> {
    if !self.reaches(Reach::Terminal) && self.on_render_path() {
      return Err(self.browser_only(made.span, "`Intl.Segmenter` on a render path, which splits by the browser's own Unicode tables"));
    }
    Ok(())
  }

  /// Whether `made` splits into graphemes, its granularity unset or `"grapheme"`.
  pub(crate) fn splits_graphemes(&self, made: &js::NewExpression) -> bool {
    let granularity = made.arguments.get(1).map(|options| match crate::syntax::arg_expr(options) {
      js::Expression::ObjectExpression(obj) => obj.properties.iter().find_map(|prop| match prop {
        js::ObjectPropertyKind::ObjectProperty(prop) => match crate::syntax::prop_of(prop) {
          crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("granularity") => Some(match &kv.value {
            js::Expression::StringLiteral(s) => crate::lossy(s.value),
            _ => String::new(),
          }),
          _ => None,
        },
        _ => None,
      }),
      _ => Some(String::new()),
    });
    matches!(granularity.flatten().as_deref(), None | Some("grapheme"))
  }

  /// `collator.compare` as a value, a function of two strings, when `collator` is an `Intl.Collator`.
  pub(crate) fn collator_compare(&mut self, member: &js::MemberExpression) -> Lowered<Option<Expr>> {
    if !matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "compare") {
      return Ok(None);
    }
    let Some(made) = self.formatter_of(member.object()).filter(|made| platform_object(made) == Some("Collator")) else { return Ok(None) };
    let (a, b) = (self.temp(), self.temp());
    let body = self.intl_call("Collator", &made.arguments, vec![Expr::Var(a.clone()), Expr::Var(b.clone())], made.span)?;
    Ok(Some(Expr::Lambda { params: vec![a, b], body: Box::new(body) }))
  }

  /// A formatter's call as the `intl` member that answers it the same way under the locale written where it is made, else the request's: `format` of `Intl.NumberFormat`, `Intl.DateTimeFormat`, `Intl.ListFormat` and `Intl.RelativeTimeFormat`, `select` of `Intl.PluralRules` and `compare` of `Intl.Collator`.
  pub(crate) fn intl_call(&mut self, formatter: &str, made: &[js::Argument], mut values: Vec<Expr>, span: Span) -> Lowered<Expr> {
    let wants = match formatter {
      "Collator" | "RelativeTimeFormat" => 2,
      _ => 1,
    };
    if values.len() < wants {
      return Err(self.residue(span, format!("`Intl.{formatter}` called with {} of its {wants} arguments", values.len())));
    }
    values.truncate(wants);
    if matches!(formatter, "NumberFormat" | "DateTimeFormat" | "ListFormat") {
      return self.intl_format(formatter, made, values.remove(0), span);
    }
    let (locale, options) = self.intl_made(formatter, made)?;
    let (name, allowed): (&str, &[(&str, &[&str])]) = match formatter {
      "PluralRules" => ("plural", &[("type", &["cardinal", "ordinal"])]),
      "Collator" => ("compare", &[("sensitivity", &["base", "accent", "case", "variant"]), ("numeric", &[]), ("usage", &["sort"]), ("ignorePunctuation", &[]), ("caseFirst", &["upper", "lower", "false"])]),
      "DurationFormat" => ("duration", &[("style", &["long", "short", "narrow", "digital"])]),
      "DisplayNames" => ("displayNames", &[("type", &["language", "region", "script", "currency", "calendar", "dateTimeField"]), ("style", &["long", "short", "narrow"]), ("languageDisplay", &["dialect", "standard"]), ("fallback", &["code", "none"])]),
      _ => ("relative", &[("numeric", &["always", "auto"]), ("style", &["long", "short", "narrow"])]),
    };
    let mut entries: Vec<Entry> = locale.into_iter().collect();
    entries.extend(self.intl_options(formatter, &options, allowed, span)?);
    values.push(Expr::Object(entries));
    Ok(Expr::Ext { module: "intl".to_owned(), name: name.to_owned(), args: values })
  }

  /// The locale and the options a formatter is made with: the locale `None` for the request's.
  pub(crate) fn intl_made<'m>(&mut self, formatter: &str, made: &'m [js::Argument<'m>]) -> Lowered<(Option<Entry>, Vec<(String, &'m js::Expression<'m>)>)> {
    let locale = match made.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
      None => None,
      Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => None,
      Some(js::Expression::StringLiteral(s)) => Some(Expr::lit_str(crate::lossy(s.value))),
      Some(other) => Some(self.expr(other)?),
    };
    let mut options: Vec<(String, &'m js::Expression<'m>)> = Vec::new();
    if let Some(given) = made.get(1) {
      let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(given) else { return Err(self.residue(crate::syntax::arg_expr(given).span(), format!("`Intl.{formatter}` options that are not an object literal"))) };
      for prop in &obj.properties {
        let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.residue(obj.span, format!("a spread in `Intl.{formatter}` options"))) };
        let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.residue(obj.span, format!("an `Intl.{formatter}` option written other than `key: value`"))) };
        let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), format!("a computed `Intl.{formatter}` option")))?;
        options.push((key, &kv.value));
      }
    }
    Ok((locale.map(|tag| Entry::Field("locale".to_owned(), tag)), options))
  }

  /// `options` as entries, each key one `allowed` names; a string written in place must be one of the key's values when it lists any.
  pub(crate) fn intl_options(&mut self, formatter: &str, options: &[(String, &js::Expression)], allowed: &[(&str, &[&str])], span: Span) -> Lowered<Vec<Entry>> {
    let mut entries = Vec::new();
    for (key, v) in options {
      let Some((_, values)) = allowed.iter().find(|(k, _)| k == key) else {
        let read = allowed.iter().map(|(k, _)| format!("`{k}`")).collect::<Vec<_>>();
        return Err(self.residue(span, format!("the `Intl.{formatter}` option `{key}`; {} are read", list_and(&read))));
      };
      if let js::Expression::StringLiteral(s) = v {
        let text = crate::lossy(s.value);
        if !values.is_empty() && !values.contains(&text.as_str()) {
          let read = values.iter().map(|v| format!("`\"{v}\"`")).collect::<Vec<_>>();
          return Err(self.residue(v.span(), format!("`{key}: \"{text}\"` in `Intl.{formatter}`; {} are read", list_and(&read))));
        }
      }
      entries.push(Entry::Field(key.clone(), self.expr(v)?));
    }
    Ok(entries)
  }

  /// The instant `value` moved by the offset of the zone `name`, which written as UTC reads as the time in that zone.
  fn shifted_into(&mut self, value: Expr, name: &str) -> Expr {
    let at = self.temp();
    let offset = Expr::Ext { module: "time".to_owned(), name: "offsetIn".to_owned(), args: vec![Expr::Var(at.clone()), Expr::lit_str(name)] };
    let shifted = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(at.clone())), Box::new(Expr::Arith(ArithOp::Mul, Box::new(offset), Box::new(Expr::Lit(Lit::Float(60_000.0))))));
    Expr::Block(vec![Stmt::Let { name: at, expr: Expr::Num(Box::new(value)) }, Stmt::Return(shifted)])
  }

  /// `toLocaleString(locale, { timeZone })` and `toLocaleTimeString(locale, { timeZone })` of a date in the zone the options name: the numeric date and the time. For `mode` `"time"` it is the time alone. `None` when the options name anything but a `timeZone` written in place.
  pub(crate) fn date_in_zone(&mut self, made: &[js::Argument], value: Expr, mode: &str) -> Lowered<Option<Expr>> {
    let Some(js::Expression::ObjectExpression(obj)) = made.get(1).map(|a| strip_parens(crate::syntax::arg_expr(a))) else { return Ok(None) };
    let [js::ObjectPropertyKind::ObjectProperty(prop)] = &obj.properties[..] else { return Ok(None) };
    let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Ok(None) };
    let (Some("timeZone"), js::Expression::StringLiteral(zone)) = (prop_name(&kv.key, kv.computed).as_deref(), &kv.value) else { return Ok(None) };
    let (locale, _) = self.intl_made("DateTimeFormat", &made[..1])?;
    Ok(Some(self.numeric_in_zone(value, &crate::lossy(zone.value), locale, mode)))
  }

  /// `intl.numeric` of `value` in the zone `zone`, `UTC` or an IANA name.
  fn numeric_in_zone(&mut self, value: Expr, zone: &str, locale: Option<Entry>, mode: &str) -> Expr {
    let value = if zone == "UTC" { value } else { self.shifted_into(value, zone) };
    let tag = match locale {
      Some(Entry::Field(_, tag)) => tag,
      _ => Expr::Lit(Lit::Null),
    };
    Expr::Ext { module: "intl".to_owned(), name: "numeric".to_owned(), args: vec![value, tag, Expr::lit_str(mode)] }
  }

  /// `new Intl.NumberFormat(locale, options).format(value)`, `new Intl.DateTimeFormat(locale, options).format(value)` and `new Intl.ListFormat(locale, options).format(items)`.
  pub(crate) fn intl_format(&mut self, formatter: &str, made: &[js::Argument], value: Expr, span: Span) -> Lowered<Expr> {
    let (locale_entry, options) = self.intl_made(formatter, made)?;
    let literal = |options: &[(String, &js::Expression)], key: &str| options.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
      js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
      _ => None,
    });
    match formatter {
      "NumberFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        if literal(&options, "style").as_deref() == Some("currency") {
          let Some((_, code)) = options.iter().find(|(k, _)| k == "currency") else { return Err(self.residue(span, "a currency style with no `currency`")) };
          let code = self.expr(code)?;
          // The browser's default display is the symbol.
          let display = match literal(&options, "currencyDisplay").as_deref() {
            None | Some("symbol") => "symbol",
            Some("code") => "code",
            Some("narrowSymbol") => "narrowSymbol",
            Some(other) => return Err(self.residue(span, format!("`currencyDisplay: \"{other}\"`; code, symbol and narrowSymbol are read"))),
          };
          entries.push(Entry::Field("display".to_owned(), Expr::lit_str(display)));
          for (key, v) in &options {
            match key.as_str() {
              "style" | "currency" | "currencyDisplay" => {}
              "minimumFractionDigits" | "maximumFractionDigits" => entries.push(Entry::Field(key.clone(), self.expr(v)?)),
              other => return Err(self.residue(span, format!("the `Intl.NumberFormat` option `{other}` with a currency; `minimumFractionDigits` and `maximumFractionDigits` are read"))),
            }
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "currency".to_owned(), args: vec![value, code, Expr::Object(entries)] });
        }
        let allowed: &[(&str, &[&str])] = &[
          ("style", &["decimal", "percent"]),
          ("minimumFractionDigits", &[]),
          ("maximumFractionDigits", &[]),
          ("minimumIntegerDigits", &[]),
          ("useGrouping", &["auto", "always", "min2"]),
          ("signDisplay", &["auto", "always", "exceptZero", "negative", "never"]),
        ];
        entries.extend(self.intl_options("NumberFormat", &options, allowed, span)?);
        Ok(Expr::Ext { module: "intl".to_owned(), name: "number".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      "ListFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        entries.extend(self.intl_options("ListFormat", &options, &[("type", &["conjunction", "disjunction", "unit"]), ("style", &["long", "short", "narrow"])], span)?);
        Ok(Expr::Ext { module: "intl".to_owned(), name: "list".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      _ => {
        if let [(key, js::Expression::StringLiteral(zone))] = &options[..] {
          if key == "timeZone" {
            return Ok(self.numeric_in_zone(value, &crate::lossy(zone.value), locale_entry, "date"));
          }
        }
        let named = literal(&options, "timeZone").filter(|zone| zone != "UTC");
        let options: Vec<(String, &js::Expression)> = if named.is_some() { options.into_iter().filter(|(k, _)| k != "timeZone").collect() } else { options };
        let zone_given = named.is_some() || options.iter().any(|(k, _)| k == "timeZone");
        if options.iter().any(|(k, v)| k == "timeZone" && !matches!(v, js::Expression::StringLiteral(_))) {
          return Err(self.residue(span, "a `timeZone` that is not written in place"));
        }
        // A named zone reads the same on the server and in the browser: the instant moves by that zone's offset and is written as UTC.
        let value = match &named {
          Some(name) => self.shifted_into(value, name),
          None => value,
        };
        // A body formats in this machine's zone, as node does: the instant moves by its offset and is written as UTC.
        let local = !zone_given && (self.reaches(Reach::Terminal) || !self.on_render_path());
        if local && options.is_empty() {
          let tag = match locale_entry {
            Some(Entry::Field(_, tag)) => tag,
            _ => Expr::Lit(Lit::Null),
          };
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "localDate".to_owned(), args: vec![value, tag, Expr::Lit(Lit::Bool(false))] });
        }
        let value = if local {
          let at = self.temp();
          let offset = Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![Expr::Var(at.clone())] };
          let shifted = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(at.clone())), Box::new(Expr::Arith(ArithOp::Mul, Box::new(offset), Box::new(Expr::Lit(Lit::Float(60_000.0))))));
          Expr::Block(vec![Stmt::Let { name: at, expr: Expr::Num(Box::new(value)) }, Stmt::Return(shifted)])
        } else {
          value
        };
        if let Some((_, time_style)) = options.iter().find(|(k, _)| k == "timeStyle") {
          if !zone_given && !local {
            return Err(self.browser_only(span, "a time of day without `timeZone: \"UTC\"`, which the browser formats in the viewer's time zone"));
          }
          if let js::Expression::StringLiteral(s) = time_style {
            let text = crate::lossy(s.value);
            if !matches!(text.as_str(), "short" | "medium") {
              return Err(self.residue(time_style.span(), format!("`timeStyle: \"{text}\"`, which names the time zone; `timeStyle: \"short\"` and `timeStyle: \"medium\"` are read")));
            }
          }
          let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
          for (key, v) in &options {
            match key.as_str() {
              "timeZone" => {}
              "timeStyle" => entries.push(Entry::Field("timeStyle".to_owned(), self.expr(v)?)),
              "dateStyle" => entries.push(Entry::Field("style".to_owned(), self.expr(v)?)),
              other => return Err(self.residue(span, format!("the `Intl.DateTimeFormat` option `{other}` beside `timeStyle`; `dateStyle` and `timeZone: \"UTC\"` are read"))),
            }
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(entries)] });
        }
        if let Some(style) = literal(&options, "dateStyle") {
          if let Some((other, _)) = options.iter().find(|(k, _)| !matches!(k.as_str(), "dateStyle" | "timeZone")) {
            return Err(self.residue(span, format!("the `Intl.DateTimeFormat` option `{other}` beside `dateStyle`; `timeStyle` and `timeZone: \"UTC\"` are read")));
          }
          return Ok(match locale_entry {
            None => Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::lit_str(style)] },
            Some(locale) => Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(vec![Entry::Field("style".to_owned(), Expr::lit_str(style)), locale])] },
          });
        }
        // The parts the server formats through a field set: a text month or
        // weekday, a numeric year and day.
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        for (key, v) in &options {
          let text = match v {
            js::Expression::StringLiteral(s) => crate::lossy(s.value),
            _ => return Err(self.residue(v.span(), format!("a `{key}` that is not written in place"))),
          };
          let read = match (key.as_str(), text.as_str()) {
            ("timeZone", _) => continue,
            ("weekday" | "month", "long" | "short" | "narrow") => true,
            ("year" | "day", "numeric") => true,
            ("month", "numeric" | "2-digit") => return Err(self.residue_with(v.span(), "a numeric month", "`month: \"short\"` or a `dateStyle`; the server has no digits that agree with the browser's")),
            ("weekday" | "month" | "year" | "day", _) => false,
            _ => return Err(self.residue(span, format!("the date formatting option `{key}`; `dateStyle`, `timeStyle`, `weekday`, `year`, `month`, `day` and `timeZone: \"UTC\"` are read"))),
          };
          if !read {
            return Err(self.residue(v.span(), format!("`{key}: \"{text}\"`")));
          }
          entries.push(Entry::Field(key.clone(), Expr::lit_str(text)));
        }
        if !entries.iter().any(|e| matches!(e, Entry::Field(k, _) if matches!(k.as_str(), "weekday" | "year" | "month" | "day"))) {
          if options.is_empty() {
            return Err(self.browser_only(span, "a date formatted with no options, which reads the viewer's time zone and locale"));
          }
          return Err(self.residue_with(span, "a date formatted without a `dateStyle` or a part", "`{ dateStyle: \"medium\" }` or `{ weekday: \"long\" }` names a format the server and the browser write the same way"));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
    }
  }

  /// `formatter.formatToParts(value)` of an `Intl.NumberFormat`, an `Intl.DateTimeFormat`, an `Intl.ListFormat` or an `Intl.RelativeTimeFormat`: the call that formats it, written in parts.
  pub(crate) fn format_to_parts(&mut self, formatter: &str, made: &js::NewExpression, call: &js::CallExpression) -> Lowered<Expr> {
    let mut values = Vec::with_capacity(call.arguments.len().max(1));
    for a in &call.arguments {
      values.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    if values.is_empty() {
      values.push(Expr::Lit(Lit::Undefined));
    }
    let formatted = self.intl_call(formatter, &made.arguments, values, made.span)?;
    match formatted {
      Expr::Ext { module, name, args } if module == "intl" && name == "date" => Ok(Expr::Ext { module, name: "dateParts".to_owned(), args }),
      Expr::Ext { module, name, args } if module == "intl" && name == "list" => Ok(Expr::Ext { module, name: "listParts".to_owned(), args }),
      Expr::Ext { module, name, args } if module == "intl" && name == "relative" => Ok(Expr::Ext { module, name: "relativeParts".to_owned(), args }),
      Expr::Ext { module, name, args } if module == "intl" && name == "numeric" && matches!(args.get(2), Some(Expr::Lit(Lit::Str(mode))) if mode == "date") => Ok(Expr::Ext { module, name: "numericParts".to_owned(), args }),
      Expr::Ext { module, name, args } if module == "intl" && matches!(name.as_str(), "number" | "currency") => {
        let options = args.last().cloned().unwrap_or(Expr::Object(Vec::new()));
        let style = match name.as_str() {
          "currency" => "currency",
          _ => match self.intl_made(formatter, &made.arguments)?.1.iter().find(|(k, _)| k == "style").map(|(_, v)| *v) {
            Some(js::Expression::StringLiteral(s)) if crate::lossy(s.value) == "percent" => "percent",
            _ => "decimal",
          },
        };
        let text = Expr::Ext { module, name, args };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "numberParts".to_owned(), args: vec![text, Expr::lit_str(style), options] })
      }
      _ => Err(self.residue_with(call.span, format!("`formatToParts` of an `Intl.{formatter}` with no options in a body"), "give it a `dateStyle` or the parts it writes")),
    }
  }

  /// `formatter.resolvedOptions()` of an `Intl.NumberFormat` or an `Intl.DateTimeFormat`: the locale it resolved to, the Gregorian calendar and the Latin digits the build formats in, the options written where it is made and, for a date, its time zone.
  pub(crate) fn resolved_options(&mut self, formatter: &str, made: &js::NewExpression) -> Lowered<Expr> {
    let (locale, options) = self.intl_made(formatter, &made.arguments)?;
    if formatter == "DurationFormat" && options.iter().any(|(key, _)| !matches!(key.as_str(), "style" | "numberingSystem" | "fractionalDigits" | "localeMatcher")) {
      return Err(self.residue(made.span, "`resolvedOptions()` of an `Intl.DurationFormat` given a unit's own style or display; `style`, `numberingSystem` and `fractionalDigits` are read"));
    }
    let body = self.reaches(Reach::Terminal) || !self.on_render_path();
    let locale = match locale {
      Some(Entry::Field(_, tag)) => tag,
      _ if body => Expr::Ext { module: "intl".to_owned(), name: "systemLocale".to_owned(), args: Vec::new() },
      _ => Expr::Ext { module: "intl".to_owned(), name: "localeTag".to_owned(), args: Vec::new() },
    };
    let mut entries = vec![Entry::Field("locale".to_owned(), locale)];
    if formatter == "DateTimeFormat" && !options.iter().any(|(k, _)| k == "timeZone") {
      if !body {
        return Err(self.browser_only(made.span, "`resolvedOptions()` of a date formatter with no `timeZone`, which names the viewer's time zone"));
      }
      entries.push(Entry::Field("timeZone".to_owned(), Expr::Ext { module: "time".to_owned(), name: "zone".to_owned(), args: Vec::new() }));
    }
    for (key, value) in options {
      entries.push(Entry::Field(key, self.expr(value)?));
    }
    Ok(Expr::Ext { module: "intl".to_owned(), name: "resolvedOptions".to_owned(), args: vec![Expr::lit_str(formatter), Expr::Object(entries)] })
  }
}
