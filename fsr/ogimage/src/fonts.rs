use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

use rustybuzz::ttf_parser;
use unicode_script::Script;

const INTER_REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular-Latin.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../fonts/Inter-Bold-Latin.ttf");

/// Why a font was not added.
#[derive(Debug, thiserror::Error, PartialEq)]
#[non_exhaustive]
pub enum FontError {
  #[error("the WOFF2 file does not decode: {0}")]
  Woff2(String),
  #[error("WOFF 1 files are not read; give the TTF, OTF or WOFF2 file instead")]
  Woff1,
  #[error("the file is not a TrueType or OpenType font")]
  NotAFont,
  #[error("`{0}` is not a Unicode script name; scripts are named as Unicode writes them, such as `Han` or `Arabic`")]
  UnknownScript(String),
}

/// A face [`Fonts`] holds, as read from the file's own tables.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FaceInfo {
  pub family: String,
  pub weight: u16,
  pub italic: bool,
  /// The scripts the face was added for, by their Unicode names.
  pub scripts: Vec<String>,
  /// Whether the face is the bundled Inter.
  pub bundled: bool,
}

#[derive(Clone)]
enum Bytes {
  Static(&'static [u8]),
  Owned(Arc<Vec<u8>>),
}

impl Bytes {
  fn as_slice(&self) -> &[u8] {
    match self {
      Bytes::Static(b) => b,
      Bytes::Owned(b) => b,
    }
  }
}

static NEXT_FACE: AtomicU64 = AtomicU64::new(1);

/// The most glyph rasters [`Fonts`] keeps. Past it the cache starts over.
const RASTERS: usize = 8192;

/// A glyph at one size, color and quarter-pixel offset.
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) struct RasterKey {
  pub face: u64,
  pub id: u16,
  pub size: u32,
  pub color: [u8; 4],
  pub fx: u8,
  pub fy: u8,
}

/// A glyph drawn once into its own pixmap, placed by its top left pixel
/// relative to the glyph origin's whole pixel.
pub(crate) struct Raster {
  pub left: i32,
  pub top: i32,
  pub pixmap: tiny_skia::Pixmap,
}

pub(crate) type Rasters = Mutex<HashMap<RasterKey, Option<Arc<Raster>>>>;

#[derive(Clone)]
struct Face {
  /// Unique across every `Fonts`, so a raster cached for one face is never
  /// read for another.
  uid: u64,
  bytes: Bytes,
  index: u32,
  family: String,
  key: String,
  weight: u16,
  italic: bool,
  scripts: Vec<Script>,
  bundled: bool,
}

/// The faces a card's text is drawn with. The bundled Inter, cut to Latin at
/// weights 400 and 700, always comes after every face added.
#[derive(Clone)]
pub struct Fonts {
  added: Vec<Face>,
  bundled: Vec<Face>,
  rasters: Arc<Rasters>,
}

impl Default for Fonts {
  fn default() -> Self {
    Fonts::new()
  }
}

impl std::fmt::Debug for Fonts {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_list().entries(self.faces()).finish()
  }
}

impl Fonts {
  pub fn new() -> Fonts {
    let bundled = [INTER_REGULAR, INTER_BOLD].into_iter().flat_map(|b| read(Bytes::Static(b), &[], true).expect("the bundled Inter reads")).collect();
    Fonts { added: Vec::new(), bundled, rasters: Arc::new(Mutex::new(HashMap::new())) }
  }

  /// Adds every face in a TTF, OTF, TTC or WOFF2 file, in the order the file
  /// holds them, after the faces already added.
  pub fn add(&mut self, bytes: impl Into<Vec<u8>>) -> Result<Vec<FaceInfo>, FontError> {
    self.add_for_scripts(bytes, &[])
  }

  /// Adds a file's faces as the ones to use for text in `scripts`, named as
  /// Unicode names them: `Han`, `Hiragana`, `Arabic`.
  pub fn add_for_scripts(&mut self, bytes: impl Into<Vec<u8>>, scripts: &[&str]) -> Result<Vec<FaceInfo>, FontError> {
    let scripts: Vec<Script> = scripts.iter().map(|s| Script::from_full_name(s).or_else(|| Script::from_short_name(s)).ok_or_else(|| FontError::UnknownScript((*s).to_owned()))).collect::<Result<_, _>>()?;
    let bytes = bytes.into();
    let bytes = match bytes.get(..4) {
      Some(b"wOF2") => wuff::decompress_woff2(&bytes).map_err(|e| FontError::Woff2(format!("{e:?}")))?,
      Some(b"wOFF") => return Err(FontError::Woff1),
      _ => bytes,
    };
    let faces = read(Bytes::Owned(Arc::new(bytes)), &scripts, false)?;
    let info = faces.iter().map(Face::info).collect();
    self.added.extend(faces);
    Ok(info)
  }

  /// Every face in the order fallback tries them.
  pub fn faces(&self) -> Vec<FaceInfo> {
    self.added.iter().chain(&self.bundled).map(Face::info).collect()
  }

  pub(crate) fn open(&self) -> Faces<'_> {
    let list = self.added.iter().chain(&self.bundled).filter_map(|f| Some(Open { face: rustybuzz::Face::from_slice(f.bytes.as_slice(), f.index)?, meta: f })).collect();
    Faces { list, rasters: &self.rasters }
  }
}

impl Face {
  fn info(&self) -> FaceInfo {
    FaceInfo { family: self.family.clone(), weight: self.weight, italic: self.italic, scripts: self.scripts.iter().map(|s| s.full_name().to_owned()).collect(), bundled: self.bundled }
  }
}

fn read(bytes: Bytes, scripts: &[Script], bundled: bool) -> Result<Vec<Face>, FontError> {
  let data = bytes.as_slice();
  let count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
  let mut faces = Vec::new();
  for index in 0..count {
    let face = ttf_parser::Face::parse(data, index).map_err(|_| FontError::NotAFont)?;
    let family = name(&face, 16).or_else(|| name(&face, 1)).unwrap_or_default();
    faces.push(Face {
      uid: NEXT_FACE.fetch_add(1, Ordering::Relaxed),
      bytes: bytes.clone(),
      index,
      key: family.to_lowercase(),
      family,
      weight: face.weight().to_number(),
      italic: face.is_italic() || face.is_oblique(),
      scripts: scripts.to_vec(),
      bundled,
    });
  }
  Ok(faces)
}

fn name(face: &ttf_parser::Face<'_>, id: u16) -> Option<String> {
  face.names().into_iter().filter(|n| n.name_id == id).find_map(|n| n.to_string())
}

pub(crate) struct Open<'a> {
  pub face: rustybuzz::Face<'a>,
  meta: &'a Face,
}

/// The faces of one render, parsed once.
pub(crate) struct Faces<'a> {
  pub list: Vec<Open<'a>>,
  rasters: &'a Rasters,
}

/// How far a face's weight is from the one asked for, in the order CSS
/// font matching prefers: lower is better.
fn weight_rank(want: u16, have: u16) -> (u8, u16) {
  let (w, h) = (want as i32, have as i32);
  if (400..=500).contains(&w) {
    if h >= w && h <= 500 {
      (0, (h - w) as u16)
    } else if h < w {
      (1, (w - h) as u16)
    } else {
      (2, (h - w) as u16)
    }
  } else if w < 400 {
    if h <= w { (0, (w - h) as u16) } else { (1, (h - w) as u16) }
  } else if h >= w {
    (0, (h - w) as u16)
  } else {
    (1, (w - h) as u16)
  }
}

impl<'a> Faces<'a> {
  pub fn uid(&self, face: usize) -> u64 {
    self.list[face].meta.uid
  }

  /// The cached raster for `key`, made with `make` on first use.
  pub fn raster(&self, key: RasterKey, make: impl FnOnce() -> Option<Raster>) -> Option<Arc<Raster>> {
    if let Some(found) = self.rasters.lock().get(&key) {
      return found.clone();
    }
    let made = make().map(Arc::new);
    let mut rasters = self.rasters.lock();
    if rasters.len() >= RASTERS {
      rasters.clear();
    }
    rasters.insert(key, made.clone());
    made
  }

  pub fn covers(&self, face: usize, c: char) -> bool {
    c.is_control() || self.list[face].face.glyph_index(c).is_some()
  }

  /// The best face of `candidates` for the weight and style, among those
  /// covering `c`.
  fn best(&self, candidates: impl Iterator<Item = usize>, c: char, weight: u16, italic: bool) -> Option<usize> {
    candidates.filter(|&i| self.covers(i, c)).min_by_key(|&i| {
      let m = self.list[i].meta;
      (m.italic != italic, weight_rank(weight, m.weight))
    })
  }

  /// The face for `c`: the element's families in order, then a face added
  /// for the character's script, then each other family in the order added,
  /// then the bundled Inter.
  pub fn select(&self, families: &[String], weight: u16, italic: bool, c: char) -> Option<usize> {
    for family in families {
      let key = family.to_lowercase();
      if let Some(i) = self.best((0..self.list.len()).filter(|&i| self.list[i].meta.key == key), c, weight, italic) {
        return Some(i);
      }
    }
    let script = unicode_script::UnicodeScript::script(&c);
    if script != Script::Common && script != Script::Inherited {
      if let Some(i) = self.best((0..self.list.len()).filter(|&i| self.list[i].meta.scripts.contains(&script)), c, weight, italic) {
        return Some(i);
      }
    }
    let mut seen: Vec<&str> = Vec::new();
    for i in 0..self.list.len() {
      let key = self.list[i].meta.key.as_str();
      if seen.contains(&key) {
        continue;
      }
      seen.push(key);
      if let Some(found) = self.best((i..self.list.len()).filter(|&j| self.list[j].meta.key == key), c, weight, italic) {
        return Some(found);
      }
    }
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_bundled_inter_reads_at_both_weights() {
    let faces = Fonts::new().faces();
    assert_eq!(faces.len(), 2);
    assert!(faces.iter().all(|f| f.family == "Inter" && f.bundled));
    assert_eq!(faces.iter().map(|f| f.weight).collect::<Vec<_>>(), vec![400, 700]);
  }

  #[test]
  fn a_weight_picks_the_nearest_face_the_way_css_does() {
    let fonts = Fonts::new();
    let faces = fonts.open();
    let weight = |w| faces.list[faces.select(&["Inter".into()], w, false, 'a').unwrap()].meta.weight;
    assert_eq!(weight(400), 400);
    assert_eq!(weight(500), 400);
    assert_eq!(weight(600), 700);
    assert_eq!(weight(900), 700);
    assert_eq!(weight(300), 400);
  }

  #[test]
  fn a_character_no_face_covers_has_no_face() {
    let fonts = Fonts::new();
    let faces = fonts.open();
    assert!(faces.select(&[], 400, false, 'a').is_some());
    assert!(faces.select(&[], 400, false, '東').is_none());
  }

  #[test]
  fn a_face_added_for_a_script_draws_that_scripts_text_before_the_other_faces() {
    let fraunces = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../cli/tests/fixtures/Fraunces-vietnamese.woff2")).unwrap();
    let mut fonts = Fonts::new();
    fonts.add(INTER_REGULAR.to_vec()).unwrap();
    fonts.add_for_scripts(fraunces, &["Latin"]).unwrap();
    let faces = fonts.open();
    let family = |c| faces.list[faces.select(&[], 400, false, c).unwrap()].meta.family.clone();
    assert_eq!(family('Ơ'), "Fraunces", "the face for Latin, though Inter was added first");
    assert_eq!(faces.list[faces.select(&["Inter".into()], 400, false, 'a').unwrap()].meta.family, "Inter", "an element's own family comes first");
  }

  #[test]
  fn garbage_and_unknown_scripts_are_refused() {
    let mut fonts = Fonts::new();
    assert_eq!(fonts.add(vec![0u8; 64]), Err(FontError::NotAFont));
    assert_eq!(fonts.add(b"wOFFxxxxxxxx".to_vec()), Err(FontError::Woff1));
    assert_eq!(fonts.add_for_scripts(INTER_REGULAR.to_vec(), &["Klingon"]), Err(FontError::UnknownScript("Klingon".into())));
  }
}
