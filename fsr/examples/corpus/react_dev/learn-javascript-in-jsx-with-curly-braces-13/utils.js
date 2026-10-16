export function getImageUrl(person) {
  return (
    'https://react.dev/images/docs/scientists/' +
    person.imageId +
    person.imageSize +
    '.jpg'
  );
}
