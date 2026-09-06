export function CrateLinks({ github, crate, className }: { github: string; crate?: string; className: string }) {
  return (
    <>
      <a href={github} className={className} target="_blank" rel="noreferrer">
        View on GitHub
      </a>
      {crate ? (
        <a href={crate} className={className} target="_blank" rel="noreferrer">
          crates.io
        </a>
      ) : null}
    </>
  );
}
