export function level(name: string): string {
  if (name === "page") return "alert alert-page";
  if (name === "warn") return "alert alert-warn";
  return "alert alert-info";
}
