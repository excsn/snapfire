export default function Loading() {
  return (
    <main className="page product">
      <div className="product-layout">
        <div className="skeleton skeleton-thumb" />
        <div className="product-info">
          <div className="skeleton skeleton-line skeleton-title" />
          <div className="skeleton skeleton-line" />
          <div className="skeleton skeleton-line skeleton-short" />
        </div>
      </div>
    </main>
  );
}
