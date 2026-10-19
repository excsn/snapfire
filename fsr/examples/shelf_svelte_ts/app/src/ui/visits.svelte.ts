/** The books opened in this tab, newest first, shared by every island that imports it. Browser state: the server never sees it. */
export const recent: { ids: string[] } = $state({ ids: [] });

export function visit(id: string): void {
  recent.ids = [id, ...recent.ids.filter((held) => held !== id)].slice(0, 3);
}
