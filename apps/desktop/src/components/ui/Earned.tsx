import type { ReactNode } from "react";

export interface EarnedProps {
  /// Whether this surface has real data behind it right now.
  when: boolean;
  children: ReactNode;
  /// Rendered instead when `when` is false. Defaults to nothing -- per the
  /// redesign plan's "earned, not requested" principle, a surface with no
  /// data behind it is *absent*, not shown empty. Only pass a fallback for
  /// the rare surface that's meant to look different-but-present at zero
  /// (the Unfiled lane, which is loud at >=1 and simply isn't earned at 0
  /// captures at all -- so even that case doesn't usually need one).
  fallback?: ReactNode;
}

/// The single implementation of "earned, not requested": every surface in
/// the redesign that only exists once real data justifies it (the project
/// chip, the session strip, Across, hold-to-aim, branch chips, the
/// capacity meter, the permission-upgrade card) gates through this rather
/// than each surface re-deriving its own "is there anything to show"
/// check and rendering its own empty state. An absent session strip and a
/// session strip with a hollow "no sessions yet" placeholder read
/// completely differently to a user -- only the former is honest at the
/// sparse data tier, and that's the default this component enforces.
export function Earned({ when, children, fallback = null }: EarnedProps) {
  return when ? children : fallback;
}
