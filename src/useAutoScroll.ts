import { useEffect, useRef, type RefObject } from "react";

/**
 * Auto-scroll a container to the bottom when deps change,
 * unless the user has scrolled up.
 */
export function useAutoScroll(
  ref: RefObject<HTMLElement | null>,
  deps: unknown[],
): void {
  const autoScroll = useRef(true);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      autoScroll.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, ref]);

  useEffect(() => {
    if (autoScroll.current && ref.current) {
      ref.current.scrollTop = ref.current.scrollHeight;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
