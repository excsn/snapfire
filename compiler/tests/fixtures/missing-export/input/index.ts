import { real, notExported } from './real.js';
import { alsoReal, own } from './barrel.js';

export const total: number = real + notExported + alsoReal + own;
