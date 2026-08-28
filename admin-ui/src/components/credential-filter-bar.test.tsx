import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CredentialFilterBar } from './credential-filter-bar'
import type { CredentialFilters } from '@/lib/credential-query-url'
import { SUBSCRIPTION_TITLE_UNKNOWN, type CredentialFacetsResponse } from '@/types/api'

/** 与组件内的防抖时长保持一致 */
const TEXT_DEBOUNCE_MS = 300

const facets: CredentialFacetsResponse = {
  subscriptionTitles: ['KIRO FREE', 'KIRO PRO', 'KIRO PRO+'],
  authMethods: ['social', 'idc', 'external_idp', 'api_key'],
}

/** 受控组件的宿主：把上报的筛选写回 props，才能测出连续改动的行为 */
function Harness({ onChange }: { onChange: (next: CredentialFilters) => void }) {
  const [filters, setFilters] = useState<CredentialFilters>({})
  return (
    <CredentialFilterBar
      filters={filters}
      facets={facets}
      onChange={(next) => {
        setFilters(next)
        onChange(next)
      }}
    />
  )
}

function renderBar() {
  const onChange = vi.fn()
  render(<Harness onChange={onChange} />)
  return { onChange, user: userEvent.setup() }
}

describe('CredentialFilterBar 筛选下发', () => {
  it('选订阅等级后按 facets 原值上报', async () => {
    const { onChange, user } = renderBar()

    await user.click(screen.getByRole('button', { name: /全部等级/ }))
    await user.click(await screen.findByRole('menuitem', { name: 'KIRO PRO+' }))

    expect(onChange).toHaveBeenCalledWith({ subscriptionTitle: 'KIRO PRO+' })
  })

  it('选「无订阅等级」下发哨兵值而不是空串', async () => {
    const { onChange, user } = renderBar()

    await user.click(screen.getByRole('button', { name: /全部等级/ }))
    await user.click(await screen.findByRole('menuitem', { name: '无订阅等级' }))

    expect(onChange).toHaveBeenCalledWith({ subscriptionTitle: SUBSCRIPTION_TITLE_UNKNOWN })
  })

  it('布尔维度下发布尔值，选回「全部」时清除该维度', async () => {
    const { onChange, user } = renderBar()

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅已禁用' }))
    expect(onChange).toHaveBeenLastCalledWith({ disabled: true })

    await user.click(screen.getByRole('button', { name: /仅已禁用/ }))
    await user.click(await screen.findByRole('menuitem', { name: '全部状态' }))
    expect(onChange).toHaveBeenLastCalledWith({ disabled: undefined })
  })

  it('优先级只接受非负整数，非法输入按缺省下发', async () => {
    const { onChange, user } = renderBar()

    await user.type(screen.getByLabelText('优先级下限'), '3')
    expect(onChange).toHaveBeenLastCalledWith({ priorityMin: 3 })

    await user.clear(screen.getByLabelText('优先级下限'))
    expect(onChange).toHaveBeenLastCalledWith({ priorityMin: undefined })
  })
})

describe('CredentialFilterBar 文本防抖', () => {
  it('连续输入三个字符只下发一次', async () => {
    const { onChange, user } = renderBar()

    await user.type(screen.getByLabelText('按 email 筛选'), 'abc')

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1))
    expect(onChange).toHaveBeenCalledWith({ email: 'abc', id: undefined })

    // 再放一个防抖周期，确认没有补发的尾巴
    await new Promise((resolve) => setTimeout(resolve, TEXT_DEBOUNCE_MS + 100))
    expect(onChange).toHaveBeenCalledTimes(1)
  })

  it('清空文本后下发缺省而不是空串', async () => {
    const { onChange, user } = renderBar()

    await user.type(screen.getByLabelText('按 id 筛选'), '12')
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith({ email: undefined, id: '12' }))

    await user.clear(screen.getByLabelText('按 id 筛选'))
    await waitFor(() =>
      expect(onChange).toHaveBeenLastCalledWith({ email: undefined, id: undefined }),
    )
  })
})

describe('CredentialFilterBar 可选值来源', () => {
  it('订阅等级下拉覆盖 facets 全集并额外提供哨兵项', async () => {
    const { user } = renderBar()

    await user.click(screen.getByRole('button', { name: /全部等级/ }))
    const items = (await screen.findAllByRole('menuitem')).map((node) => node.textContent)

    expect(items).toEqual(['全部等级', '无订阅等级', 'KIRO FREE', 'KIRO PRO', 'KIRO PRO+'])
  })

  it('认证方式下拉覆盖 facets 全集，含 external_idp', async () => {
    const { user } = renderBar()

    await user.click(screen.getByRole('button', { name: /全部类型/ }))
    const items = (await screen.findAllByRole('menuitem')).map((node) => node.textContent)

    expect(items).toHaveLength(1 + facets.authMethods.length)
    expect(items).toContain('外部 IdP')
  })

  it('facets 未就绪时只渲染固定项，不报错', () => {
    render(<CredentialFilterBar filters={{}} onChange={vi.fn()} />)
    expect(screen.getByRole('button', { name: /全部等级/ })).toBeInTheDocument()
  })
})

describe('CredentialFilterBar 清空', () => {
  it('无条件时不渲染清空按钮，有条件后一键清空', async () => {
    const { onChange, user } = renderBar()
    expect(screen.queryByRole('button', { name: '清空筛选' })).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅已禁用' }))

    await user.click(screen.getByRole('button', { name: '清空筛选' }))
    expect(onChange).toHaveBeenLastCalledWith({})
  })
})
