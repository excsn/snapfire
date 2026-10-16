import LikeButton from './LikeButton';

let count = 0;

export default function App() {
  async function addLike() {
    'use server';
    count++;
  }

  return (
    <div>
      <h1>Inline Server Actions</h1>
      <p>Likes: {count}</p>
      <LikeButton addLike={addLike} />
    </div>
  );
}
