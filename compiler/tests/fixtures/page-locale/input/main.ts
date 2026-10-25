export const a = new Intl.NumberFormat().format(1234.5);
export const b = new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(0);
export const c = Intl.Collator().compare("a", "b");
export const d = (1234.5).toLocaleString();
export const e = new Date(0).toLocaleDateString(undefined, { dateStyle: "short" });
export const f = "a".localeCompare("b");
export const g = new Intl.NumberFormat("de-DE").format(1);
export const h = (1).toLocaleString("fr-FR");
export const i = "a".localeCompare("b", "sv");
export function shadowed(Intl: { NumberFormat: new () => unknown }) {
  return new Intl.NumberFormat();
}
