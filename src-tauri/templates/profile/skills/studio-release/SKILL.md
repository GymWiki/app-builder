---
name: studio-release
description: Prepare this Studio app for release: version, build number, changelog and what is still missing on the publish checklist. Use when the user asks to release, publish, upload, bump the version or write a changelog.
---
# Release preparation
- `get_checklist` shows what is done and what is missing. `bump_version` raises the version in pubspec.yaml and Studio together (patch for fixes, minor for new features).
- Write `CHANGELOG.md` in plain language for app users: short bullets, newest version first.
- You never upload or publish. Studio does that after the owner confirms; do not run store or signing commands.
- Do not touch signing files; Studio manages the keystore and passwords.
