import { useOptimistic, startTransition } from 'react';

export default function FollowButton({ user, followAction }) {
  const [optimisticState, updateOptimistic] = useOptimistic(
    { isFollowing: user.isFollowing, followerCount: user.followerCount },
    (current, isFollowing) => ({
      isFollowing,
      followerCount: current.followerCount + (isFollowing ? 1 : -1)
    })
  );

  function handleClick() {
    const newFollowState = !optimisticState.isFollowing;
    startTransition(async () => {
      updateOptimistic(newFollowState);
      await followAction(newFollowState);
    });
  }

  return (
    <div>
      <p><strong>{user.name}</strong></p>
      <p>{optimisticState.followerCount} followers</p>
      <button onClick={handleClick}>
        {optimisticState.isFollowing ? 'Unfollow' : 'Follow'}
      </button>
    </div>
  );
}
