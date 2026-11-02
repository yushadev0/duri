import { invoke } from "@tauri-apps/api/core";
import { createContext, useContext } from "react";

export interface IslandControls {
  /** Collapses the island to its resting state. */
  collapse: () => void;
  /**
   * Takes keyboard focus for a text field and holds the island open, or
   * gives focus back to the app the person was using.
   */
  setTyping: (typing: boolean) => void;
}

export const IslandContext = createContext<IslandControls>({
  collapse: () => {},
  setTyping: () => {},
});

export function useIsland() {
  return useContext(IslandContext);
}

export function setIslandFocus(focused: boolean) {
  return invoke("island_set_focus", { focused }).catch(() => {});
}
