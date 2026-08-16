export enum Color {
  Red = 'red',
  Blue = 'blue',
}

export namespace Shapes {
  export const square = 'square';
}

export class Point {
  constructor(private readonly x: number, public readonly y: number) {}
}
