export interface NameInput {
  name: string;
}

export interface PlayInput {
  wave: string;
  cell: bigint;
}

export interface ResetInput {
  wave: string;
}

export interface BlipInput {
  wave: string;
  parent: string;
  body: string;
}

export interface AmendInput {
  wave: string;
  blip: string;
  body: string;
}
