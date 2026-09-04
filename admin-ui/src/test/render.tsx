import type { ReactElement, ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render } from '@testing-library/react'
import type { CredentialStatusItem } from '@/types/api'

/** 测试用 QueryClient：关重试与后台刷新，避免用例间互相干扰 */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, gcTime: 0 },
      mutations: { retry: false },
    },
  })
}

/** 带 QueryClientProvider 的 render */
export function renderWithQuery(ui: ReactElement) {
  const queryClient = createTestQueryClient()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return { queryClient, ...render(ui, { wrapper }) }
}

/** 凭据列表项的最小可用构造，按需覆盖字段 */
export function makeCredential(
  overrides: Partial<CredentialStatusItem> = {},
): CredentialStatusItem {
  return {
    id: 1,
    priority: 0,
    disabled: false,
    failureCount: 0,
    isCurrent: false,
    expiresAt: null,
    authMethod: 'social',
    hasProfileArn: true,
    successCount: 0,
    lastUsedAt: null,
    hasProxy: false,
    refreshFailureCount: 0,
    endpoint: 'default',
    ...overrides,
  }
}
