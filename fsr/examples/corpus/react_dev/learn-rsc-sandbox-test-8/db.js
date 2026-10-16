let todos = ['Buy groceries'];

export function getTodos() {
  return [...todos];
}

export function addTodo(text) {
  todos.push(text);
}
