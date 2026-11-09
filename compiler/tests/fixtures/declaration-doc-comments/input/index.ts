/** Adds two numbers. */
export function add(a: number, b: number): number {
  return a + b;
}

/**
 * A counter that starts at zero.
 * Each call to `bump` adds one.
 */
export class Counter {
  /** How many times `bump` ran. */
  count = 0;

  /** Adds one to `count`. */
  bump(): void {
    this.count += 1;
  }
}

export interface Options {
  /** The first value. */
  start: number;
}
