import type { MDXComponents } from "mdx/types";
import type { Children } from "@snapfire/fsr-authoring/template";

/** A second-level heading with a link to itself. */
function Heading({ id, className, children }: { id?: string; className?: string; children?: Children }) {
  if (className !== undefined) {
    return (
      <h2 id={id} className={className}>
        {children}
      </h2>
    );
  }
  return (
    <h2 id={id} className="mt-6 mb-2 font-serif text-xl">
      <a href={`#${id}`} aria-hidden="true" className="mr-1 text-ink-soft">#</a>
      {children}
    </h2>
  );
}

/** A quotation set off with a rule. */
function Quote({ children }: { children?: Children }) {
  return <blockquote className="my-4 border-l-4 border-accent pl-4 italic text-ink-soft">{children}</blockquote>;
}

export function useMDXComponents(components?: MDXComponents): MDXComponents {
  return { h2: Heading, blockquote: Quote, ...components };
}
