export async function toggleLike(value) {
  return await new Promise((res) => setTimeout(() => res(value), 1000));
  // In a real app, this would update the server
}
