// Presence helpers shared by the chat header and the inbox rows.

import type { AppState, UserShort } from "../../state";

/** Presence entries older than this stop showing green: the backend polls the
 * presence feeds every ~60 s, so two missed polls mean the entry is stale. */
const PRESENCE_STALE_MS = 2 * 60 * 1000;

/** True when `userId` has a fresh `active` presence entry. */
export function presenceActive(presence: AppState["presence"], userId: string): boolean {
  const entry = presence[userId];
  if (!entry?.active) return false;
  return entry.at <= 0 || Date.now() - entry.at <= PRESENCE_STALE_MS;
}

/** Active participants of a thread (the viewer is not in `users` normally,
 * but is filtered out anyway). */
export function activeParticipants(
  presence: AppState["presence"],
  users: UserShort[],
  viewerId: string,
): UserShort[] {
  return users.filter((u) => u.pk !== viewerId && presenceActive(presence, u.pk));
}
