import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from './dialog'

/**
 * jsdom 不做布局计算，真实几何观测不到，判据只能是静态类名。
 * 这里锁住的是弹窗可用性的必要条件：高度上限与滚动通路必须同时存在，
 * 只有其中一个时 grid 行仍按内容撑开，footer 反而比改前更远离视口。
 */
function renderContent(className?: string) {
  render(
    <Dialog open>
      <DialogContent className={className}>
        <DialogTitle>标题</DialogTitle>
        <DialogDescription>说明</DialogDescription>
      </DialogContent>
    </Dialog>
  )
  return screen.getByRole('dialog')
}

describe('DialogContent 基类', () => {
  it('同时含高度上限与纵向滚动，缺一则改动无效', () => {
    const tokens = renderContent().className.split(' ')
    expect(tokens).toContain('max-h-[90vh]')
    expect(tokens).toContain('overflow-y-auto')
  })

  it('宽度留出左右边距，让 sm:rounded-lg 在窄屏可见', () => {
    const tokens = renderContent().className.split(' ')
    expect(tokens).toContain('w-[calc(100%-2rem)]')
    expect(tokens).not.toContain('w-full')
  })

  it('调用方的 max-h 覆盖基类值，不产生两个高度上限', () => {
    const tokens = renderContent('max-h-[85vh]').className.split(' ')
    expect(tokens).toContain('max-h-[85vh]')
    expect(tokens).not.toContain('max-h-[90vh]')
  })

  it('调用方的 overflow-y-visible 抵消基类滚动，避免双滚动条', () => {
    const tokens = renderContent('flex flex-col overflow-y-visible').className.split(' ')
    expect(tokens).toContain('overflow-y-visible')
    expect(tokens).not.toContain('overflow-y-auto')
  })
})
