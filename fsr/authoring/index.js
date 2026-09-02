export function fail(kind, message) {
  const error = new Error(message);
  error.kind = kind;
  throw error;
}

export function action(body) {
  return body;
}
