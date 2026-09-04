import { describe, expect, it, vi } from 'vitest'
import { screen } from '@testing-library/react'
import { renderWithQuery } from '@/test/render'
import { AddCredentialDialog } from './add-credential-dialog'
import { OnlineAuthDialog } from './online-auth-dialog'
import { KamImportDialog } from './kam-import-dialog'
import { BatchImportDialog } from './batch-import-dialog'

vi.mock('@/api/credentials', () => ({
  getCredentials: vi.fn().mockResolvedValue({
    total: 0,
    available: 0,
    disabled: 0,
    credentials: [],
    page: 1,
    perPage: 12,
    totalPages: 0,
    filteredTotal: 0,
  }),
  addCredential: vi.fn(),
  importCredential: vi.fn(),
  importCredentialsBatch: vi.fn(),
  importKamDocument: vi.fn(),
  startBuilderIdLogin: vi.fn(),
  pollBuilderIdLogin: vi.fn(),
  startIamSsoLogin: vi.fn(),
  completeIamSsoLogin: vi.fn(),
  importSsoToken: vi.fn(),
}))

vi.mock('@/api/settings', () => ({
  getEndpointSettings: vi.fn().mockResolvedValue({
    defaultEndpoint: 'ide',
    registeredEndpoints: ['ide'],
  }),
}))

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  Toaster: () => null,
}))

/**
 * DialogContent 基类带了 overflow-y-auto，用 flex flex-col 配内层滚动区的弹窗
 * 会多出一层外层滚动条。这几个必须显式写 overflow-y-visible 抵消。
 * 基类改动与抵消类是一组，缺任一侧都会退化，所以两边都断言。
 */
const cases = [
  ['添加凭据', AddCredentialDialog],
  ['在线授权', OnlineAuthDialog],
  ['KAM 导入', KamImportDialog],
  ['批量导入', BatchImportDialog],
] as const

describe('内层滚动弹窗抵消基类滚动', () => {
  it.each(cases)('%s 含 overflow-y-visible 且无外层滚动', (_name, Component) => {
    renderWithQuery(<Component open onOpenChange={vi.fn()} />)
    const tokens = screen.getByRole('dialog').className.split(' ')
    expect(tokens).toContain('overflow-y-visible')
    expect(tokens).not.toContain('overflow-y-auto')
    expect(tokens).toContain('flex')
    expect(tokens).not.toContain('grid')
  })
})
