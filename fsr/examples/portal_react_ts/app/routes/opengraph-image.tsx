import type { RootProps } from "@generated/client";

export const size = { width: 1200, height: 630 };
export const alt = "Acme portal: the teams and the sites they own";

export default function Image({ teams }: RootProps) {
  return (
    <div className="flex h-full w-full flex-col justify-between p-[72px] text-white" style={{ backgroundImage: "linear-gradient(135deg, #0b1020 0%, #1d2b64 100%)" }}>
      <div className="flex items-center gap-5">
        <div className="size-14 rounded-[14px] bg-amber-500" />
        <span className="text-[34px] font-bold">Acme portal</span>
      </div>
      <h1 className="m-0 text-[76px]/[1.1] font-bold">{teams.length} teams, one portal</h1>
      <p className="m-0 text-[30px] opacity-75">{teams.map((team) => team.name).join(" · ")}</p>
    </div>
  );
}
