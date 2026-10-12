use crate::date::Date;
use crate::error::{Error, Result};
use crate::writer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
  pub name: String,
  /// ISO 639 language code. `zh-cn` and `zh-tw` are the two accepted
  /// exceptions to the two-or-three-letter rule.
  pub language: String,
}

/// A `<news:news>` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct News {
  publication: Publication,
  publication_date: Date,
  title: String,
}

impl News {
  pub fn new(publication: Publication, publication_date: impl Into<Date>, title: impl Into<String>) -> Self {
    Self {
      publication,
      publication_date: publication_date.into(),
      title: title.into(),
    }
  }

  pub fn title(&self) -> &str {
    &self.title
  }

  pub(crate) fn write(&self, out: &mut String, depth: usize) {
    let inner = depth + 1;

    writer::open(out, depth, "news:news");
    writer::open(out, inner, "news:publication");
    writer::tag(out, inner + 1, "news:name", &self.publication.name);
    writer::tag(out, inner + 1, "news:language", &self.publication.language);
    writer::close(out, inner, "news:publication");
    writer::tag(out, inner, "news:publication_date", &self.publication_date.render());
    writer::tag(out, inner, "news:title", &self.title);
    writer::close(out, depth, "news:news");
  }
}

pub(crate) fn check_count(count: usize) -> Result<()> {
  if count > crate::limit::MAX_NEWS_PER_URL_SET {
    return Err(Error::TooMuchNews { count });
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  use chrono::NaiveDate;

  #[test]
  fn a_news_entry_nests_its_publication() {
    let mut out = String::new();

    News::new(
      Publication {
        name: "Norman's Oven".to_string(),
        language: "en".to_string(),
      },
      NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
      "A title",
    )
    .write(&mut out, 0);

    assert_eq!(
      out,
      "<news:news>\n  <news:publication>\n    <news:name>Norman&apos;s Oven</news:name>\n    \
       <news:language>en</news:language>\n  </news:publication>\n  \
       <news:publication_date>2026-06-01</news:publication_date>\n  <news:title>A title</news:title>\n</news:news>\n"
    );
  }
}
