import { useEffect, useRef } from "react";

/** Scrolls `el` by `speed` px/s while `playing`, frame-rate independent. */
export function useAutoScroll(el: HTMLElement | null, playing: boolean, speed: number) {
  const last = useRef<number | null>(null);
  const carry = useRef(0);

  useEffect(() => {
    if (!el || !playing) {
      last.current = null;
      return;
    }
    let raf = 0;
    const tick = (t: number) => {
      if (last.current !== null) {
        carry.current += ((t - last.current) / 1000) * speed;
        const whole = Math.floor(carry.current);
        if (whole > 0) {
          el.scrollTop += whole;
          carry.current -= whole;
        }
      }
      last.current = t;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [el, playing, speed]);
}
