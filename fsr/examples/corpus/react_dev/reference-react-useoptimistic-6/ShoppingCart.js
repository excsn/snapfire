import { useOptimistic, startTransition } from 'react';

export default function ShoppingCart({ cart, cartActions }) {
  const [optimisticCart, dispatch] = useOptimistic(
    cart,
    (currentCart, action) => {
      switch (action.type) {
        case 'add':
          const exists = currentCart.find(item => item.id === action.item.id);
          if (exists) {
            return currentCart.map(item =>
              item.id === action.item.id
                ? { ...item, quantity: item.quantity + 1, pending: true }
                : item
            );
          }
          return [...currentCart, { ...action.item, quantity: 1, pending: true }];
        case 'remove':
          return currentCart.filter(item => item.id !== action.id);
        case 'update_quantity':
          return currentCart.map(item =>
            item.id === action.id
              ? { ...item, quantity: action.quantity, pending: true }
              : item
          );
        default:
          return currentCart;
      }
    }
  );

  function handleAdd(item) {
    startTransition(async () => {
      dispatch({ type: 'add', item });
      await cartActions.add(item);
    });
  }

  function handleRemove(id) {
    startTransition(async () => {
      dispatch({ type: 'remove', id });
      await cartActions.remove(id);
    });
  }

  function handleUpdateQuantity(id, quantity) {
    startTransition(async () => {
      dispatch({ type: 'update_quantity', id, quantity });
      await cartActions.updateQuantity(id, quantity);
    });
  }

  const total = optimisticCart.reduce(
    (sum, item) => sum + item.price * item.quantity,
    0
  );

  return (
    <div>
      <h2>Shopping Cart</h2>
      <div style={{ marginBottom: 16 }}>
        <button onClick={() => handleAdd({
          id: 1, name: 'T-Shirt', price: 25
        })}>
          Add T-Shirt ($25)
        </button>{' '}
        <button onClick={() => handleAdd({
          id: 2, name: 'Mug', price: 15
        })}>
          Add Mug ($15)
        </button>
      </div>
      {optimisticCart.length === 0 ? (
        <p>Your cart is empty</p>
      ) : (
        <ul>
          {optimisticCart.map(item => (
            <li key={item.id}>
              {item.name} - ${item.price} ×
              {item.quantity}
              {' '}= ${item.price * item.quantity}
              <button
                onClick={() => handleRemove(item.id)}
                style={{ marginLeft: 8 }}
              >
                Remove
              </button>
              {item.pending && ' ...'}
            </li>
          ))}
        </ul>
      )}
      <p><strong>Total: ${total}</strong></p>
    </div>
  );
}
