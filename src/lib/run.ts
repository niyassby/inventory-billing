/**
 * An event handler that starts async work (React handlers must not return promises). Errors are
 * the async function's own business: handle them inside it.
 */
export function run(fn: () => Promise<unknown>): () => void {
  return () => {
    void fn();
  };
}
