export interface Session {
  visits: number;
  /** A theme the visitor picked on the account page; empty follows each route's own. */
  theme: string;
  /** `light`, `dark` or empty for none. */
  scheme: string;
}

export const defaults: Session = {
  visits: 0,
  theme: "",
  scheme: "",
};

/** The header's scheme button: `light`, `dark` or empty. */
export interface SchemeChoice {
  scheme: string;
}

/** The header's theme picker: a theme's name or empty. */
export interface ThemeChoice {
  theme: string;
}

/** The account page's look form, as the browser posts it. */
export interface LookForm {
  theme: string;
  scheme: string;
}
