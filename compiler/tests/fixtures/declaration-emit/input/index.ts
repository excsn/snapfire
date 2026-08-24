import { state } from './state';
import { pkg } from 'some-package';

export * from './state';
export { state as reexported } from './state';

export const current: number = state.count;
export const label = (prefix: string): string => `${prefix}${pkg}`;
