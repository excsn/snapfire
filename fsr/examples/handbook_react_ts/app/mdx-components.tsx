import type { ReactNode } from "react";
import type { MDXComponents } from "mdx/types";

/** A second-level heading with a link to itself; in the FAQ, one question. */
function Heading({ id, className, children }: { id?: string; className?: string; children?: ReactNode }) {
  if (className !== undefined) {
    return (
      <h2 id={id} className={className}>
        {children}
      </h2>
    );
  }
  return (
    <h2 id={id} className="qa">
      <a href={`#${id}`} aria-hidden="true">#</a> {children}
    </h2>
  );
}

export function useMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    h2: Heading,
    wrapper: ({ children }: { children?: ReactNode }) => <div className="page prose">{children}</div>,
    ...components,
  };
}
