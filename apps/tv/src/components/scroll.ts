import { useRef } from 'react';

// The first row to draw in a list of `count` rows showing `visible` at a
// time. The list only scrolls once the highlighted row comes within
// `margin` rows of an edge, like a native list.
export function nextFirstVisible(
  top: number,
  index: number,
  count: number,
  visible: number,
  margin = 2,
): number {
  let next = top;
  if (index < next + margin) {
    next = index - margin;
  } else if (index > next + visible - 1 - margin) {
    next = index - (visible - 1 - margin);
  }
  return Math.max(0, Math.min(next, count - visible));
}

export function useFirstVisible(
  index: number,
  count: number,
  visible: number,
): number {
  const top = useRef(0);
  top.current = nextFirstVisible(top.current, index, count, visible);
  return top.current;
}
