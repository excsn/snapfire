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

/** The account page's look form, as the browser posts it. */
export interface LookForm {
  theme: string;
  scheme: string;
}
