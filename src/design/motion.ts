import type { Transition } from "motion/react";

/** Shape changes of the island: quick, with a small physical overshoot. */
export const morphSpring: Transition = { type: "spring", visualDuration: 0.42, bounce: 0.24 };

/** Collapsing settles without bounce so it doesn't feel like it is "hopping". */
export const settleSpring: Transition = { type: "spring", visualDuration: 0.34, bounce: 0.08 };

/** Used instead of springs when the user prefers reduced motion. */
export const reducedTransition: Transition = { duration: 0.14, ease: "easeOut" };

/** Content cross-fade inside the island. */
export const contentIn: Transition = { duration: 0.24, delay: 0.05, ease: [0.2, 0.8, 0.2, 1] };
export const contentOut: Transition = { duration: 0.12, ease: "easeIn" };
