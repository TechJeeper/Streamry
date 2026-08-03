import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";

/** Open an edit modal when navigated with `?id=` (e.g. from activity log). */
export function useOpenFromQuery<T extends { id: string }>(
  items: T[],
  open: (item: T) => void,
) {
  const [params, setParams] = useSearchParams();
  useEffect(() => {
    const id = params.get("id");
    if (!id || items.length === 0) return;
    const found = items.find((i) => i.id === id);
    if (!found) return;
    open(found);
    const next = new URLSearchParams(params);
    next.delete("id");
    setParams(next, { replace: true });
  }, [items, params, setParams, open]);
}
