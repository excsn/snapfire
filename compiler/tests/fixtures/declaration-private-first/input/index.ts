/** The values read so far, by key. Not part of the module's surface. */
const cache = new Map<string, number>();

/** Reads the value stored under `key`. */
export function read(key: string): number | undefined {
  return cache.get(key);
}
