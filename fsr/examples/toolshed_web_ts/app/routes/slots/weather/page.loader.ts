import type { Ctx } from "@snapfire/fsr/host";

export async function load({ services }: Ctx) {
  const weather = await services.shed.getWeather();
  return { weather };
}
