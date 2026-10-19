# AGENTS.md — Meteor Blitz port

## Ground rules

- **No PII in commits or files.** Author identity is `BobClawblaw <BobClawblaw@users.noreply.github.com>`.
- **Serial execution.** One task at a time. No parallel subagent fan-out.
- **Commit after every task.** Conventional commits.
- **Worklog.** Append to `worklog/YYYY-MM-DD.md` before finishing a session: what changed, what was verified with real output, what is next, blockers. Append only.
- **Plan first.** Work from `docs/port-plan.md`; update it when scope changes.

## Asset provenance

- `original/` is read-only. The IPA, the extracted `.app`, decoded textures, and decoded audio are source material.
- Derived assets go in `assets/` and must be regenerable by a script in `tools/`.
- Do not hand-edit decoded PNGs or WAVs; fix the decode script and rerun.

## Repos

- Primary: `nas` → `ssh://agentnas/volume1/storage/git/meteor-blitz.git`
- Secondary mirror: `github` → `BobClawblaw/meteor-blitz` (private)
- Push to both. Never force-push without listing what dies first.

## Windows builds

Run on `xamd4090` (SSH alias, key auth installed). Node 24.14 and git 2.53 present; Rust is not — install rustup before the first Windows build.
