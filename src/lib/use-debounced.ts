import { useEffect, useState } from "react";

/** Waits until typing pauses (search as you type without a query per key). */
export function useDebounced<T>(value: T, ms = 150): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}
