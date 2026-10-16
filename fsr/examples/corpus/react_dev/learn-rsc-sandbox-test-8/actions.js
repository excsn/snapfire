'use server';
import { addTodo } from './db';

export async function createTodo(text) {
  if (!text) return 'Please enter a todo.';
  addTodo(text);
  return 'Added: ' + text;
}
