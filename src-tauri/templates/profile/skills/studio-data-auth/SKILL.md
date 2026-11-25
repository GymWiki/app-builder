---
name: studio-data-auth
description: Add data storage or user accounts to this Studio app with the owner's own Supabase, Firebase or a local database, including login screens and account deletion. Use for requests about saving data, syncing, login, sign-up or privacy of user data.
---
# Data and login
- Read `.studio/publish.json` (`data.provider`, `data.authMethods`) for the owner's choice. Do not switch provider without asking.
- All data access goes in `lib/core/data/` (a repository class per kind of data, exposed through a Riverpod provider). Screens never talk to the backend directly.
- Supabase: use the `supabase` MCP server (if connected) to create tables and row level security; ask before destructive changes. Firebase: FlutterFire packages and the `firebase` MCP server. Local: `sqflite` or `drift`.
- Keys: only the public anon/web config may be in code. Service keys and tokens never go in files; ask the owner to use Studio's data screen.
- Accounts require a working "delete my account" function (store rule). Details: references/accounts.md.
