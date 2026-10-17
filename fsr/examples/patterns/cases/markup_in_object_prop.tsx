function Tile({ tile }: { tile: { title: string; icon: any } }) {
  return <div className="tile">{tile.icon}<span>{tile.title}</span></div>;
}
export default function Page() {
  const tile = { title: "Mail", icon: <i>@</i> };
  return <Tile tile={tile} />;
}
