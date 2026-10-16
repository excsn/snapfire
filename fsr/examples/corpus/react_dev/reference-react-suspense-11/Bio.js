import { use } from 'react';

export default function Bio({ bioPromise }) {
  const bio = use(bioPromise);
  return <p>{bio}</p>;
}
