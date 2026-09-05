export default function Help() {
  return (
    <div className="page help">
      <h1>How this works</h1>
      <p>
        Every panel on this page is rendered by the server. The alert count in the header, the region in the bar and the number of agents you watch are one store the whole document
        shares, seeded by the loaders and written by whichever panel changes them.
      </p>
      <p>Nothing here is fetched by the browser. The panels arrive as one server-rendered tree, and the parts that are slower than the rest stream in behind their own placeholders.</p>
    </div>
  );
}
