import type { Ctx, DataOf, MetaCtx } from "@snapfire/fsr";
import { preloadImage } from "@snapfire/fsr/head";
import { PHOTOS } from "@src/ui/photos";

export async function load({ query, services }: Ctx<"/">) {
  const products = await services.shopping.listProducts({ q: query.q, category: query.category, tag: query.tag });
  return { products, q: query.q, category: query.category };
}

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: data.q ? `Results for ${data.q} · Shopping` : "Today's picks · Shopping",
  // The first card is the largest thing above the fold, so its photo is fetched before the stylesheet parses.
  head: data.products.length > 0 ? [preloadImage(PHOTOS[data.products[0].image.file], "(max-width: 640px) 100vw, (max-width: 1100px) 50vw, 300px")] : [],
});
