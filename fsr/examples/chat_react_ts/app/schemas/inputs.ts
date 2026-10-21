export interface NameInput {
  name: string;
  /** The page to go back to once named, a path on this site. */
  next?: string;
}

export interface SayInput {
  room: string;
  body: string;
}
