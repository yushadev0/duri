import type { Transition } from "motion/react";

/** Shape changes of the island: quick, with a small physical overshoot. */
export const morphSpring: Transition = { type: "spring", visualDuration: 0.42, bounce: 0.24 };

/** Collapsing settles without bounce so it doesn't feel like it is "hopping". */
export const settleSpring: Transition = { type: "spring", visualDuration: 0.34, bounce: 0.08 };

/** Used instead of springs when the user prefers reduced motion. */
export const reducedTransition: Transition = { duration: 0.14, ease: "easeOut" };

/**
 * Content cross-fade inside the island. Kept to opacity and a slight scale:
 * blurring the whole layer made every click look like a full repaint.
 */
export const contentIn: Transition = { duration: 0.2, delay: 0.04, ease: [0.2, 0.8, 0.2, 1] };
export const contentOut: Transition = { duration: 0.08, ease: "easeOut" };
