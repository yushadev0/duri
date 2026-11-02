import { useTranslation } from "react-i18next";
import { useNow } from "./useNow";

/** "now", "5 min", "2 h", "3 d" — refreshed every 15 s. */
export function useAgo() {
  const { t } = useTranslation();
  const now = useNow(true, 15_000);
  return (at: number) => {
    const min = Math.floor((now - at) / 60_000);
    if (min < 1) return t("sessions.now");
    if (min < 60) return `${min} ${t("timer.units.min")}`;
    const h = Math.floor(min / 60);
    if (h < 24) return `${h} ${t("timer.units.h")}`;
    return `${Math.floor(h / 24)} ${t("timer.units.d")}`;
  };
}
