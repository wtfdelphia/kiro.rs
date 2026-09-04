import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { KamImportDialog } from './kam-import-dialog'
import { renderWithQuery } from '@/test/render'
import type { KamImportResponse } from '@/api/credentials'

const api = vi.hoisted(() => ({
  importKamDocument: vi.fn(),
  getCredentials: vi.fn(),
}))
vi.mock('@/api/credentials', () => api)

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  Toaster: () => null,
}))

// 不含空格的长串，默认换行规则断不开；父级是 overflow-y-auto，横向溢出会变成滚动条
const LONG_PATH = '$.accounts[0].credentials.socialAuthProfileWithAVeryLongIdentifier0123456789'
const LONG_ERROR =
  'refreshToken 校验失败：https://oidc.us-east-1.amazonaws.com/token?client_id=aaaaaaaabbbbbbbbccccccccdddddddd'

const response: KamImportResponse = {
  success: true,
  container: 'Wrapper',
  preview: [
    {
      index: 0,
      path: LONG_PATH,
      hasRefreshToken: false,
      hasClientId: false,
      hasClientSecret: false,
      hasTokenEndpoint: false,
      hasIssuerUrl: false,
      hasScopes: false,
      hasProfileArn: false,
      disabled: false,
      valid: false,
      error: LONG_ERROR,
    },
  ],
  results: [],
}

beforeEach(() => {
  api.importKamDocument.mockReset()
  api.importKamDocument.mockResolvedValue(response)
  api.getCredentials.mockResolvedValue({
    total: 0,
    available: 0,
    disabled: 0,
    credentials: [],
    page: 1,
    perPage: 12,
    totalPages: 0,
    filteredTotal: 0,
  })
})

/** 走一次导入，让预览行、统计行与结果错误三处都渲染出来 */
async function importOnce() {
  renderWithQuery(<KamImportDialog open onOpenChange={vi.fn()} />)
  const user = userEvent.setup()
  // 用 paste 而非 type：JSON 里的 { } 会被 userEvent 当成键描述符
  await user.click(screen.getByRole('textbox'))
  await user.paste('[{"refreshToken":"x"}]')
  await user.click(screen.getByRole('button', { name: '开始导入并验活' }))
  await waitFor(() => expect(api.importKamDocument).toHaveBeenCalled())
}

describe('KamImportDialog 窄屏回流', () => {
  it('预览行的长路径允许在词内换行', async () => {
    await importOnce()
    const row = await screen.findByText(new RegExp(LONG_PATH.replace(/[$.[\]]/g, '\\$&')))
    expect(row.className.split(' ')).toContain('break-words')
  })

  it('结果区的失败原因允许在词内换行', async () => {
    await importOnce()
    const node = await screen.findByText(LONG_ERROR)
    expect(node.className.split(' ')).toContain('break-words')
  })

  it('统计行允许折行，窄屏下 5 项不横向溢出', async () => {
    await importOnce()
    const stat = await screen.findByText(/^✓ 成功:/)
    const row = stat.parentElement!
    const tokens = row.className.split(' ')
    expect(tokens).toContain('flex')
    expect(tokens).toContain('flex-wrap')
  })
})
