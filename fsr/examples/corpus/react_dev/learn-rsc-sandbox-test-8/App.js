import { getTodos } from './db';
import { createTodo } from './actions';
import AddTodo from './AddTodo';

export default function App() {
  const todos = getTodos();
  return (
    <div>
      <h1>Todo List</h1>
      <p style={{ color: '#666', fontSize: 13 }}>
        This list is rendered by a server component
        reading server-side data. It only updates because
        the server re-renders after each action.
      </p>
      <ul>
        {todos.map((todo, i) => (
          <li key={i}>{todo}</li>
        ))}
      </ul>
      <AddTodo createTodo={createTodo} />
    </div>
  );
}
