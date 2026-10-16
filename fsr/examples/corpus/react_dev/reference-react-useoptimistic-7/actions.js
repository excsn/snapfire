export async function deleteItem(id) {
  await new Promise((res) => setTimeout(res, 1000));
  // Item 3 always fails to demonstrate error recovery
  if (id === 3) {
    throw new Error('Cannot delete. Permission denied.');
  }
}
