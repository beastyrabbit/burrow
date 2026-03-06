import { useCallback, useEffect, useRef, type RefObject } from "react";

/**
 * Auto-scroll a container to the bottom when deps change,
 * unless the user has scrolled up.
 *
 * Scroll-listener registration is deferred: on each dep change, the hook
 * checks whether ref.current has been populated (e.g., a conditionally
 * rendered tab just became visible) and attaches the listener at that point.
 */
export function useAutoScroll(
  ref: RefObject<HTMLElement | null>,
  deps: unknown[],
): void {
  const autoScroll = useRef(true);
  const listenerEl = useRef<HTMLElement | null>(null);
  const onScrollRef = useRef<(() => void) | null>(null);

  const attach = useCallback((el: HTMLElement) => {
    if (listenerEl.current === el) return;
    // Remove old listener before attaching new one
    if (listenerEl.current && onScrollRef.current) {
      listenerEl.current.removeEventListener("scroll", onScrollRef.current);
    }
    listenerEl.current = el;
    const onScroll = () => {
      autoScroll.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    };
    onScrollRef.current = onScroll;
    el.addEventListener("scroll", onScroll);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (listenerEl.current && onScrollRef.current) {
        listenerEl.current.removeEventListener("scroll", onScrollRef.current);
        listenerEl.current = null;
        onScrollRef.current = null;
      }
    };
  }, []);

  // Observe ref.current changes via polling on deps (cheap — only runs when content changes)
  useEffect(() => {
    const el = ref.current;
    if (el && el !== listenerEl.current) attach(el);
    if (autoScroll.current && el) {
      el.scrollTop = el.scrollHeight;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
