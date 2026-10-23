// Errors. `throw`, `try`, `catch`, and `finally` work as in TypeScript,
// with two limits. You throw only error objects. A fault, such as a read
// outside an array, is not an error that `catch` sees. This script takes
// orders in a shop.

class Shop {
  stock: i32;
  constructor(stock: i32) {
    this.stock = stock;
  }

  buy(count: i32): i32 {
    // Throw an `Error`, or one of its kinds: `RangeError`, `TypeError`,
    // and the others that JavaScript has. `throw "no stock"` does not
    // compile, so `catch` always gets an error object.
    if (count <= 0) {
      throw new RangeError(`count must be positive, got ${count}`);
    }
    if (count > this.stock) {
      throw new Error(`only ${this.stock} left`);
    }
    this.stock -= count;
    return this.stock;
  }
}

function order(shop: Shop, count: i32): void {
  try {
    print(`bought ${count}, ${shop.buy(count)} left`);
  } catch (e) {
    // Before you read `e.message`, ask what `e` is with `instanceof`.
    // Test the most specific kind first: a `RangeError` is also an `Error`.
    if (e instanceof RangeError) {
      print(`bad order: ${e.message}`);
    } else if (e instanceof Error) {
      print(`no sale: ${e.message}`);
    }
  } finally {
    // `finally` runs after `try` and after `catch`, as in TypeScript.
    print(`order of ${count} done`);
  }
}

export function main(): void {
  const shop = new Shop(5);
  order(shop, 3);
  order(shop, 0);
  order(shop, 4);

  // A fault is different from an exception. A read outside an array, an
  // integer division by zero, or a read of a freed object stops the
  // program at once. No `catch` sees it, and no `finally` runs. The host
  // application gets the reason and the source position.
}
