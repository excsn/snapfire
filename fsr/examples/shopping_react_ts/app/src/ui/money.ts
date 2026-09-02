export function money(cents: bigint | number): string {
  return `$${(Number(cents) / 100).toFixed(2)}`;
}

export function count(n: bigint | number): number {
  return Number(n);
}

export function percentOff(price: bigint | number, list: bigint | number | null | undefined): number {
  if (list === undefined || list === null || Number(list) <= Number(price)) return 0;
  return Math.round((1 - Number(price) / Number(list)) * 100);
}
