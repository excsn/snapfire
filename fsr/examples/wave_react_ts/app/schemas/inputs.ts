export interface NameInput {
  name: string;
}

export interface PlayInput {
  wave: string;
  blip: string;
  /** The gadget block the board is. */
  block: string;
  cell: bigint;
}

export interface ResetInput {
  wave: string;
  blip: string;
  block: string;
}

export interface VoteInput {
  wave: string;
  blip: string;
  /** The gadget block the vote is. */
  block: string;
  answer: string;
}

export interface CloseInput {
  wave: string;
  blip: string;
  /** The gadget block the vote is. */
  block: string;
}

export interface BlipInput {
  wave: string;
  parent: string;
  /** The block of the parent this answers, empty for the whole blip. */
  anchor: string;
  body: string;
}

export interface AmendInput {
  wave: string;
  blip: string;
  /** The block rewritten, empty for the whole blip. */
  block: string;
  body: string;
}
