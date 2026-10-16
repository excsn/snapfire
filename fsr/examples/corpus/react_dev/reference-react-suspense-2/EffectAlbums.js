import { useState, useEffect } from 'react';
import { fetchData } from './data.js';

export default function EffectAlbums({ artistId }) {
  const [albums, setAlbums] = useState([]);

  useEffect(() => {
    let active = true;
    fetchData(`/${artistId}/albums`).then(result => {
      if (active) {
        setAlbums(result);
      }
    });
    return () => {
      active = false;
    };
  }, [artistId]);

  // Suspense can't see this fetch, so its fallback never
  // shows. The list stays empty until the data arrives.
  return (
    <ul>
      {albums.map(album => (
        <li key={album.id}>
          {album.title} ({album.year})
        </li>
      ))}
    </ul>
  );
}
