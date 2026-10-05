/** How often each probe rendered and was unmounted, by owner, which the specs read off the window. */
export interface Tally {
  renders: Record<string, number>;
  unmounts: Record<string, number>;
}

export function tally(): Tally {
  const scope = globalThis as { __probes?: Tally };
  scope.__probes ??= { renders: {}, unmounts: {} };
  return scope.__probes;
}

export function rendered(owner: string): void {
  const t = tally();
  t.renders[owner] = (t.renders[owner] ?? 0) + 1;
}

export function unmounted(owner: string): void {
  const t = tally();
  t.unmounts[owner] = (t.unmounts[owner] ?? 0) + 1;
}
