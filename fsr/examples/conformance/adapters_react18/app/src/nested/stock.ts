let finish: () => void = () => {};

export const counted = new Promise<number>((resolve) => {
  finish = () => resolve(12);
});

(globalThis as { finishPantry?: () => void }).finishPantry = () => finish();
