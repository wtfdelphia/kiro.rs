import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BatchVerifyDialog, type VerifyResult } from './batch-verify-dialog'

/**
 * 错误信息里常见不含空格的长串（URL、ARN、堆栈片段），默认换行规则断不开。
 * 父级列表是 overflow-y-auto，横向溢出因此计算为 auto，会在列表内部冒出横向滚动条。
 * break-words 是消除这条横向滚动的必要条件。
 */
const longError =
  'https://codewhisperer.us-east-1.amazonaws.com/generateAssistantResponse?requestId=00000000-0000-0000-0000-000000000000'

function renderWithError() {
  const results = new Map<number, VerifyResult>([
    [1, { id: 1, status: 'failed', error: longError }],
  ])
  render(
    <BatchVerifyDialog
      open
      onOpenChange={vi.fn()}
      verifying={false}
      progress={{ current: 1, total: 1 }}
      results={results}
      onCancel={vi.fn()}
    />
  )
  return screen.getByText(`错误: ${longError}`)
}

describe('BatchVerifyDialog 长错误信息', () => {
  it('错误信息节点允许在词内换行', () => {
    expect(renderWithError().className.split(' ')).toContain('break-words')
  })
})
