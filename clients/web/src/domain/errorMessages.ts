import { RosterApiError } from "../api/client";
import type { MessageKey, Translate } from "../i18n/messages";

/**
 * Map a failed operation onto a localized, teacher-facing message.
 *
 * Mirrors the RosterImportPanel approach: a stable backend code selects a
 * specific sentence, everything else (including raw transport strings and
 * parser errors) collapses into the caller's fallback key so implementation
 * details never reach the UI. The original error stays on the console for
 * diagnosis.
 */
export function describeApiError(
  error: unknown,
  t: Translate,
  fallbackKey: MessageKey,
): string {
  if (error instanceof RosterApiError) {
    switch (error.code) {
      case "session_required":
        return t("app.sessionExpired");
      case "feature_unavailable":
        return t("app.featureUnavailable");
      default:
        return t(fallbackKey);
    }
  }
  if (error instanceof Error) {
    console.error("Workbench operation failed", error);
  }
  return t(fallbackKey);
}
