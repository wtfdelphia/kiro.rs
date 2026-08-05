# Apply session: admin-ui-model-ops-entrypoints

Date: 2026-07-22

## Implemented

- Types: ModelsRefreshResponse/All, CredentialModelsResponse, TestCredential*
- API: refreshCredentialModels, refreshAllModels, getCredentialModels(live?), testCredential
- Dashboard: 刷新全部模型 + ModelsRefreshResultDialog (failed details)
- Card: 查看模型 / 刷新模型 / 测试 + dialogs
- README Admin UI entry note
- tasks.md all 17 checked

## Verification run this session

1. pnpm --dir admin-ui exec tsc -b --pretty false -> exit 0
2. pnpm --dir admin-ui exec vite build -> exit 0, dist generated
3. Static smoke: dist JS contains UI labels 刷新全部模型 / 查看模型 / 开始测试
4. openspec validate --all -> 9 passed, 0 failed

## Not run / residual risk

- Live browser click on running service (current local account may be upstream suspended; UI error path still valid)
- cargo rebuild to embed new admin-ui/dist into binary (required for /admin of packaged kiro-rs)
- Optional hooks/use-credentials mutations skipped; component-local state used instead (task 1.3 optional)

## Files touched

- admin-ui/src/types/api.ts
- admin-ui/src/api/credentials.ts
- admin-ui/src/components/dashboard.tsx
- admin-ui/src/components/credential-card.tsx
- admin-ui/src/components/credential-models-dialog.tsx (new)
- admin-ui/src/components/credential-test-dialog.tsx (new)
- admin-ui/src/components/models-refresh-result-dialog.tsx (new)
- README.md
- openspec/changes/admin-ui-model-ops-entrypoints/**
