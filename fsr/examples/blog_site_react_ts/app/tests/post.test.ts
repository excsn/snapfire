import { load, meta, paths } from "@routes/post/[slug]/page.loader";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("a post is found by its slug and titles the document", async () => {
  const c = ctx<void, "/post/{slug}">({ params: { slug: "a-site-is-a-mount" } });
  const { post } = await load(c);
  expect(post.title).toEqual("A site is a mount, not a dependency");
  const described = meta({ data: { post } });
  expect(described.title).toEqual("A site is a mount, not a dependency · Blog");
});

test("a slug off the blog is refused as not found", async () => {
  const c = ctx<void, "/post/{slug}">({ params: { slug: "nope" } });
  await expect(load(c)).rejects.toMatchObject({ kind: "not_found" });
});

test("paths names every post once", () => {
  const c = ctx({});
  const sets = paths();
  const slugs = sets.map((set) => set.slug);
  expect(slugs).toEqual(["why-a-plan-file", "a-site-is-a-mount", "static-under-a-shell"]);
});
