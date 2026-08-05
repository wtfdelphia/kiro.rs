# Optimization follow-up: profile-arn-resolution

Date: 2026-07-20 (Asia/Shanghai)
Context: openspec-apply-change after all_done; residual quality pass from verify WARN.

## Changes this session

1. **Admin credential card observability**
   - Show `provider` badge when present.
   - Always show Profile status: 已就绪 / 未解析 / 不适用 (api_key).
   - Addresses credential-import UX gap (hasProfileArn visibility beyond KAM dialog).

2. **profile.rs cleanup**
   - Simplified redundant BuilderId fixed-path nesting.
   - `list_err` assigned via match (no unused_assignments warning).
   - `parse_list_available_profiles_body` marked `pub(crate)` (pure parse contract).

3. **provider.rs cleanup**
   - Removed unused `had_profile_before` variable; resolve still soft-fails without InvalidRefreshToken.

4. **Housekeeping**
   - Updated change README status.
   - Removed ephemeral `tools/_opt*` helper scripts.

## Verification (this session)

| Command | Result |
| --- | --- |
| `cargo test --bin kiro-rs -- kiro::profile` (CARGO_TARGET_DIR under %TEMP%) | **11 passed** |
| `cargo test --bin kiro-rs -- kiro::profile kiro::model::credentials kiro::endpoint` | **61 passed** (pre-warning fix; recompile after list_err fix clean for profile) |
| `openspec validate profile-arn-resolution` | **valid** |
| `pnpm build` (admin-ui) | **SKIPPED/FAIL env**: vite esbuild `spawn EPERM` under sandbox |
| `node node_modules/typescript/bin/tsc -b` | **exit 0** (typecheck only) |

## Residual

- No HTTP mock for live ListAvailableProfiles (YAGNI; pure parse covered).
- Production IdC e2e curl not run (no secrets).
- Full workspace cargo target lock may be access-denied; used temp target dir.
- Archive still optional via openspec-archive-change.

