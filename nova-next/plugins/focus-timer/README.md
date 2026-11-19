# Focus Timer (work in progress)

A proposed first-party plugin kind for the Nova Island fork.

## Behaviour

- Start, pause, resume and reset a focus session.
- Default focus duration: 25 minutes; short break: 5 minutes.
- Show remaining time in the island and an accessible expanded control panel.
- Preserve countdown accuracy across sleep by using a monotonic deadline while running, and explicitly decide how sleep should affect sessions.
- Save only user preferences and completed-session counts; do not record activity or clipboard content.
- Provide English and Arabic UI labels and keyboard access.

## Security / installation

This `manifest.json` is a **development specification**, not an installable signed `.nvp` bundle. The upstream plugin verifier only supports `radio` and `tasks`, and requires a trusted signature. Never bypass verification or ship a private signing key in the application. The fork needs a separately generated development signing key, embedded public verification key, an updated kind allowlist, and a timer runtime/UI implementation before this can be installed.

## Acceptance criteria

1. Start 25:00; pause/resume does not lose time.
2. Reset restores configured duration; break switch uses its own duration.
3. Closing and reopening the island does not duplicate timers or notifications.
4. Invalid or unsigned bundles are rejected.
5. Unit tests cover timer transitions and duration boundaries; Windows integration tests cover notifications and scaling.

**Status:** design/manifest only. No working timer or signed package yet.
