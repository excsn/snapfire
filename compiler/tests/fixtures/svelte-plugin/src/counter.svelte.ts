export const counter: { n: number } = $state({ n: 0 });

export function bump(by: number = 1) {
  counter.n += by;
}
