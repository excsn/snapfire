/// What a stylesheet used, so a caller can skip work for one that uses none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Features(pub u8);

impl Features {
  pub const NONE: Features = Features(0);
  pub const AT_APPLY: Features = Features(1);
  pub const AT_IMPORT: Features = Features(2);
  pub const JS_PLUGIN_COMPAT: Features = Features(4);
  pub const THEME_FUNCTION: Features = Features(8);
  pub const UTILITIES: Features = Features(16);
  pub const VARIANTS: Features = Features(32);
  pub const AT_THEME: Features = Features(64);

  pub fn has(self, other: Features) -> bool {
    self.0 & other.0 != 0
  }
}

impl std::ops::BitOr for Features {
  type Output = Features;
  fn bitor(self, rhs: Features) -> Features {
    Features(self.0 | rhs.0)
  }
}

impl std::ops::BitOrAssign for Features {
  fn bitor_assign(&mut self, rhs: Features) {
    self.0 |= rhs.0;
  }
}
