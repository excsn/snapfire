import { state } from './state';
import { widget } from './widgets';
import { already } from './already.js';
import { typed } from './typed.ts';
import { pkg } from 'some-package';
import './theme.css';
export * from './state';

export const lazy = () => import('./state');
export { state, widget, already, typed, pkg };
