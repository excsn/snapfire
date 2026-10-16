export async function deliverMessage(message) {
  await new Promise((res) => setTimeout(res, 1000));
  return message;
}
