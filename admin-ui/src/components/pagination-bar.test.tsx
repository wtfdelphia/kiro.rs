import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { PaginationBar } from './pagination-bar'
import type { PageInfo } from '@/types/api'

function makePageInfo(overrides: Partial<PageInfo> = {}): PageInfo {
  return {
    page: 1,
    perPage: 12,
    filteredTotal: 240,
    totalPages: 20,
    hasPrev: false,
    hasNext: true,
    ...overrides,
  }
}

function renderBar(pageInfo: Partial<PageInfo> = {}) {
  const onPageChange = vi.fn()
  const onPerPageChange = vi.fn()
  render(
    <PaginationBar
      pageInfo={makePageInfo(pageInfo)}
      onPageChange={onPageChange}
      onPerPageChange={onPerPageChange}
    />,
  )
  return { onPageChange, onPerPageChange }
}

describe('PaginationBar 边界按钮', () => {
  it('首页时上一页渲染为禁用态而非移除', () => {
    renderBar({ page: 1, hasPrev: false, hasNext: true })

    const prev = screen.getByRole('button', { name: '上一页' })
    expect(prev).toBeInTheDocument()
    expect(prev).toBeDisabled()
    expect(screen.getByRole('button', { name: '下一页' })).toBeEnabled()
  })

  it('末页时下一页渲染为禁用态而非移除', () => {
    renderBar({ page: 20, hasPrev: true, hasNext: false })

    const next = screen.getByRole('button', { name: '下一页' })
    expect(next).toBeInTheDocument()
    expect(next).toBeDisabled()
    expect(screen.getByRole('button', { name: '上一页' })).toBeEnabled()
  })

  it('点上下页按当前页码加减一上报', async () => {
    const { onPageChange } = renderBar({ page: 5, hasPrev: true, hasNext: true })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: '上一页' }))
    await user.click(screen.getByRole('button', { name: '下一页' }))

    expect(onPageChange.mock.calls).toEqual([[4], [6]])
  })
})

describe('PaginationBar 无障碍', () => {
  it('当前页可识别：aria-current 只落在当前页码上', () => {
    renderBar({ page: 5, hasPrev: true, hasNext: true })

    expect(screen.getByRole('button', { name: 'Page 5' })).toHaveAttribute('aria-current', 'page')
    expect(screen.getByRole('button', { name: 'Page 4' })).not.toHaveAttribute('aria-current')
  })

  it('导航区有 aria-label，页码播报区为 aria-live', () => {
    renderBar({ page: 5, hasPrev: true, hasNext: true })

    expect(screen.getByRole('navigation', { name: '分页导航' })).toBeInTheDocument()
    expect(screen.getByText(/第 5 \/ 20 页（共 240 个凭据）/)).toHaveAttribute(
      'aria-live',
      'polite',
    )
  })

  it('键盘遍历跳过省略号：省略号 aria-hidden 且不进可聚焦元素序列', async () => {
    renderBar({ page: 10, hasPrev: true, hasNext: true })
    const user = userEvent.setup()

    const ellipses = screen.getAllByText('…')
    expect(ellipses).toHaveLength(2)
    ellipses.forEach((node) => expect(node).toHaveAttribute('aria-hidden', 'true'))

    // 从第 8 页起连按三次 Tab，落点应是 9、10、11 三个页码，省略号不占位
    const start = screen.getByRole('button', { name: 'Page 8' })
    start.focus()
    for (const label of ['Page 9', 'Page 10', 'Page 11']) {
      await user.tab()
      expect(screen.getByRole('button', { name: label })).toHaveFocus()
    }
  })
})

describe('PaginationBar 每页条数', () => {
  it('展示当前每页条数并可切换', async () => {
    const { onPerPageChange } = renderBar({ page: 1, perPage: 12 })
    const user = userEvent.setup()

    const trigger = screen.getByRole('button', { name: /12 条\/页/ })
    await user.click(trigger)
    await user.click(await screen.findByRole('menuitem', { name: '48 条/页' }))

    expect(onPerPageChange).toHaveBeenCalledWith(48)
  })
})
