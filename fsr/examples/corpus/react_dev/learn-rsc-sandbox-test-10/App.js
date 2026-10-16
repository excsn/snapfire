import { addLike, getLikeCount } from './actions';
import LikeButton from './LikeButton';

export default async function App() {
  const count = await getLikeCount();
  return (
    <div>
      <h1>Server Functions</h1>
      <p>Likes: {count}</p>
      <LikeButton addLike={addLike} />
    </div>
  );
}
