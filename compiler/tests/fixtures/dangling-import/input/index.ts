import { gone } from './gone.js';
import './theme.css';

export const lazy = (): Promise<unknown> => import('./absent.js');
export const value: number = gone;
