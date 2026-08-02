import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";

/// Which project the main window is scoped to, if any -- derived, never
/// defaulted. See the redesign plan's "there is no default focused
/// project": magpie already knows which project you're in once an agent
/// connects (a live session's `project_id`), so focus arrives on its own
/// rather than being guessed at launch. `null` is a first-class, common
/// state (no live session, no explicit pin) -- render as "no project chip"
/// (see the Earned primitive), not as a loading placeholder or a default
/// to some project.
///
/// An explicit pin (⌥⇥, once that gesture exists) always wins over the
/// derived signal and persists across restarts via the
/// `pinned_project_id` setting -- once you deliberately choose a project,
/// magpie stops guessing for you until you clear it.
export function useProjectSignal() {
  // `undefined` = the persisted pin hasn't loaded yet; `null` = loaded,
  // genuinely no pin. Collapsed to `null` in the returned `projectId`
  // below, but kept distinct here so `refreshLiveSessions` racing ahead of
  // the settings fetch can't be misread as "pin is absent" for a moment.
  const [pinnedProjectId, setPinnedProjectIdState] = useState<number | null | undefined>(
    undefined,
  );
  const [liveProjectIds, setLiveProjectIds] = useState<number[]>([]);

  const refreshLiveSessions = useCallback(() => {
    api
      .listSessions(null)
      .then((sessions) => {
        const distinct = Array.from(
          new Set(
            sessions
              .filter((s) => s.ended_at === null && s.project_id !== null)
              .map((s) => s.project_id as number),
          ),
        );
        setLiveProjectIds(distinct);
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    api
      .getSetting("pinned_project_id")
      .then((v) => setPinnedProjectIdState(v ? Number(v) : null))
      .catch(() => setPinnedProjectIdState(null));
    refreshLiveSessions();
  }, [refreshLiveSessions]);

  // Writing "" (not omitting the key) is how a pin is cleared -- there's
  // no delete_setting command, only get/set, so an empty string is the
  // sentinel `getSetting` round-trips back to "no pin" on the next load.
  const setPin = useCallback((projectId: number | null) => {
    setPinnedProjectIdState(projectId);
    api.setSetting("pinned_project_id", projectId === null ? "" : String(projectId)).catch(console.error);
  }, []);

  // Explicit pin wins. Otherwise: if sessions are live across exactly one
  // distinct project, that's the signal. Live across MORE than one project
  // has no single honest answer -- staying null here (no chip) is correct;
  // that's exactly what Across (⌘⌥K) is for once it exists.
  const derivedProjectId = liveProjectIds.length === 1 ? liveProjectIds[0] : null;
  const projectId =
    pinnedProjectId != null ? pinnedProjectId : pinnedProjectId === undefined ? null : derivedProjectId;

  return {
    projectId,
    pinned: pinnedProjectId != null,
    liveProjectIds,
    setPin,
    refreshLiveSessions,
  };
}
