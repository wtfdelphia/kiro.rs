# Apply session verification (profile-arn-resolution)

Date: 2026-07-20

## Commands run

| Command | Result |
| --- | --- |
| cargo test --bin kiro-rs -- kiro::profile kiro::model::credentials kiro::token_manager kiro::endpoint | **97 passed** |
| openspec validate --all | **2 passed, 0 failed** |
| pnpm build (admin-ui) | **success** (vite production build) |
| git status --short | only intended source/docs/openspec changes; no config.json/credentials.json/.codegraph staged |

## Pre-existing failures (out of scope)

Full cargo test has 8 failures in nthropic::converter (UnsupportedModel claude-sonnet-4). Not introduced by this change; filtered kiro/* tests pass.

## Evidence skills status

| Skill / artifact | Status |
| --- | --- |
| bridge-plan.md | present |
| spec-compliance-check | next gate (not run in apply) |
| openspec-verify-change | next gate |
| verification-before-completion | next gate |

## Key code touchpoints

- src/kiro/profile.rs (new)
- src/kiro/provider.rs resolve + 403 path
- src/kiro/token_manager.rs set_profile_arn / usage resolve
- src/admin/* + dmin-ui KAM import
