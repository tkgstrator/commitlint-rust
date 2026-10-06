# Existing Mac guard

Use an installed guarded Git entry point. The native installer can recognize the original known ~/.codex/git-identity-guard configuration and preserve prior hook/verification records. Never replace the active guard while a history-repair task records its hashes. Finish that task first, then run the explicitly authorized native installer and reopen shells/agents.

The native skill also works without hooks on another environment, but every actual new commit and outgoing push still requires its checks.
