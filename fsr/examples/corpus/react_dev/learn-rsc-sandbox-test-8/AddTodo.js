'use client';
import { useState, useTransition } from 'react';

export default function AddTodo({ createTodo }) {
  const [text, setText] = useState('');
  const [message, setMessage] = useState('');
  const [pending, startTransition] = useTransition();

  function handleSubmit(e) {
    e.preventDefault();
    startTransition(async () => {
      const result = await createTodo(text);
      setMessage(result);
      setText('');
    });
  }

  return (
    <div>
      <form onSubmit={handleSubmit}>
        <input
          value={text}
          onChange={e => setText(e.target.value)}
          placeholder="New todo"
        />
        <button disabled={pending}>
          {pending ? 'Adding...' : 'Add'}
        </button>
      </form>
      {message && (
        <p style={{ color: '#666', fontSize: 13 }}>
          Action returned: "{message}"
        </p>
      )}
    </div>
  );
}
