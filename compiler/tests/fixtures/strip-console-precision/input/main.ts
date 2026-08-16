class Logger {
  log(msg: string): void {}
  debug(msg: string): void {}
}

const logger = new Logger();

console.log('top level log');
console.debug('top level debug');

export const run = (): void => {
  console.log('nested log');
  console.debug('nested debug');
  console.warn('warn survives');
  logger.log('method survives');
  logger.debug('method survives');
  const captured = console.log('value survives');
  void captured;
};
