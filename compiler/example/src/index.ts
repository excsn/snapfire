import config from './data/config.json' with { type: 'json' };
import { toast } from './ui/toast';
import { formatCount } from './utils';
import './style.css';

export const ready = (count: number): void => {
  console.debug('booting', config.name);
  toast(`${config.name}: ${formatCount(count)}`);
};

export const loadEditor = () => import('./editor');
