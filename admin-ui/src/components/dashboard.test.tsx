import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Dashboard } from './dashboard'
import { makeCredential, renderWithQuery } from '@/test/render'
import type {
  BalanceResponse,
  CredentialStatusItem,
  CredentialsQuery,
  CredentialsStatusResponse,
} from '@/types/api'

const api = vi.hoisted(() => ({
  getCredentials: vi.fn(),
  getCredentialFacets: vi.fn(),
  fetchAllDisabledIds: vi.fn(),
  getCredentialBalance: vi.fn(),
  deleteCredential: vi.fn(),
  resetCredentialFailure: vi.fn(),
  forceRefreshToken: vi.fn(),
  refreshAllModels: vi.fn(),
  refreshCredentialModels: vi.fn(),
  setCredentialDisabled: vi.fn(),
  setCredentialPriority: vi.fn(),
  addCredential: vi.fn(),
  importCredential: vi.fn(),
  importCredentialsBatch: vi.fn(),
  importKamDocument: vi.fn(),
  getCredentialModels: vi.fn(),
  testCredential: vi.fn(),
  startBuilderIdLogin: vi.fn(),
  pollBuilderIdLogin: vi.fn(),
  startIamSsoLogin: vi.fn(),
  completeIamSsoLogin: vi.fn(),
  importSsoToken: vi.fn(),
  getLoadBalancingMode: vi.fn(),
  setLoadBalancingMode: vi.fn(),
}))

vi.mock('@/api/credentials', () => api)

const getEndpointSettings = vi.hoisted(() => vi.fn())
vi.mock('@/api/settings', () => ({ getEndpointSettings }))

const toast = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}))
vi.mock('sonner', () => ({ toast, Toaster: () => null }))

const DEFAULT_PER_PAGE = 12

/**
 * 内存版列表服务端：只实现本组用例需要的筛选与切页。
 *
 * 前端断言的是「范围」而不是「过滤算法」，筛选实现刻意保持最小；
 * 分页则必须真做，否则测不出跨页选中与全量计数这些关键行为。
 */
function fakeServer(all: CredentialStatusItem[]) {
  const store = [...all]

  const respond = (query: CredentialsQuery): CredentialsStatusResponse => {
    let rows = store
    if (query.disabled !== undefined) rows = rows.filter((c) => c.disabled === query.disabled)
    if (query.subscriptionTitle !== undefined) {
      rows = rows.filter((c) => c.subscriptionTitle === query.subscriptionTitle)
    }
    if (query.email) rows = rows.filter((c) => (c.email ?? '').includes(query.email!))

    const perPage = query.perPage ?? DEFAULT_PER_PAGE
    const page = query.page ?? 1
    const filteredTotal = rows.length
    const totalPages = Math.ceil(filteredTotal / perPage)
    const start = (page - 1) * perPage

    return {
      total: store.length,
      available: store.filter((c) => !c.disabled).length,
      currentId: store[0]?.id ?? 0,
      credentials: rows.slice(start, start + perPage),
      pageInfo: {
        page,
        perPage,
        filteredTotal,
        totalPages,
        hasPrev: page > 1,
        hasNext: page < totalPages,
      },
    }
  }

  api.getCredentials.mockImplementation((query: CredentialsQuery = {}) =>
    Promise.resolve(respond(query)),
  )
  api.fetchAllDisabledIds.mockImplementation(() =>
    Promise.resolve(store.filter((c) => c.disabled).map((c) => c.id)),
  )
  api.deleteCredential.mockImplementation((id: number) => {
    const index = store.findIndex((c) => c.id === id)
    if (index >= 0) store.splice(index, 1)
    return Promise.resolve({ success: true, message: '已删除' })
  })

  return { store }
}

/** 生成 n 条凭据，`disabledIds` 内的置为已禁用 */
function makeMany(n: number, options: { disabledIds?: number[]; failedIds?: number[] } = {}) {
  const disabled = new Set(options.disabledIds ?? [])
  const failed = new Set(options.failedIds ?? [])
  return Array.from({ length: n }, (_, i) => {
    const id = i + 1
    return makeCredential({
      id,
      email: `user${id}@example.com`,
      disabled: disabled.has(id),
      failureCount: failed.has(id) ? 3 : 0,
    })
  })
}

const balance: BalanceResponse = {
  id: 1,
  subscriptionTitle: 'KIRO PRO',
  currentUsage: 10,
  usageLimit: 100,
  remaining: 90,
  usagePercentage: 10,
  nextResetAt: null,
}

beforeEach(() => {
  vi.clearAllMocks()
  window.localStorage.clear()
  window.history.replaceState(null, '', '/')
  getEndpointSettings.mockResolvedValue({ defaultEndpoint: 'ide', registeredEndpoints: ['ide'] })
  api.getCredentialFacets.mockResolvedValue({
    subscriptionTitles: ['KIRO FREE', 'KIRO PRO'],
    authMethods: ['social', 'api_key'],
  })
  api.getLoadBalancingMode.mockResolvedValue({ mode: 'priority' })
  api.getCredentialBalance.mockResolvedValue(balance)
  api.refreshAllModels.mockResolvedValue({ refreshed: 0, failed: 0, globalCount: 0, results: [] })
  vi.spyOn(window, 'confirm').mockReturnValue(true)
})

/** 渲染并等首屏列表就位 */
async function renderDashboard() {
  const result = renderWithQuery(<Dashboard onLogout={vi.fn()} />)
  await screen.findByRole('heading', { name: '凭据管理' })
  return { ...result, user: userEvent.setup() }
}

/** 当前页渲染出的卡片复选框，顺序与列表一致 */
function pageCheckboxes() {
  return screen.getAllByRole('checkbox', { name: /^选择凭据 #/ })
}

function checkboxOf(id: number) {
  return screen.getByRole('checkbox', { name: `选择凭据 #${id}` })
}

describe('Dashboard 服务端分页', () => {
  it('只渲染服务端返回的当前页，不在前端切片', async () => {
    fakeServer(makeMany(30))
    await renderDashboard()

    // 30 条数据、每页 12：若还在前端 slice 完整数组，卡片数会是 30
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))
    expect(api.getCredentials).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, perPage: 12 }),
    )
  })

  it('翻页按服务端页码请求下一页数据', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))

    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    expect(screen.queryByRole('checkbox', { name: '选择凭据 #1' })).not.toBeInTheDocument()
    expect(api.getCredentials).toHaveBeenCalledWith(expect.objectContaining({ page: 2 }))
  })

  it('末页只渲染余下条数', async () => {
    fakeServer(makeMany(14))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))

    await waitFor(() => expect(pageCheckboxes()).toHaveLength(2))
  })

  it('翻页请求进行中上一页内容仍在，不闪空列表', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    // 让第 2 页的响应挂住，观察请求在途时的画面
    let release!: () => void
    const pending = new Promise<void>((resolve) => {
      release = resolve
    })
    const original = api.getCredentials.getMockImplementation()!
    api.getCredentials.mockImplementation(async (query: CredentialsQuery = {}) => {
      if (query.page === 2) await pending
      return original(query)
    })

    await user.click(screen.getByRole('button', { name: '下一页' }))

    expect(checkboxOf(1)).toBeInTheDocument()
    expect(pageCheckboxes()).toHaveLength(12)

    release()
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
  })
})

describe('Dashboard 每页条数与越界回退', () => {
  it('改每页条数回到第 1 页并写入本地存储', async () => {
    fakeServer(makeMany(60))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())

    await user.click(screen.getByRole('button', { name: /12 条\/页/ }))
    await user.click(await screen.findByRole('menuitem', { name: '24 条/页' }))

    await waitFor(() =>
      expect(api.getCredentials).toHaveBeenLastCalledWith(
        expect.objectContaining({ page: 1, perPage: 24 }),
      ),
    )
    expect(checkboxOf(1)).toBeInTheDocument()
    expect(window.localStorage.getItem('credentialsPerPage')).toBe('24')
  })

  it('下次进入页面沿用已存的每页条数', async () => {
    // 上一次会话留下的值，本次挂载时应直接生效而不是回落默认 12
    window.localStorage.setItem('credentialsPerPage', '24')
    fakeServer(makeMany(60))
    await renderDashboard()

    await waitFor(() => expect(pageCheckboxes()).toHaveLength(24))
    expect(api.getCredentials).toHaveBeenCalledWith(expect.objectContaining({ perPage: 24 }))
    expect(api.getCredentials.mock.calls.every((c) => c[0]?.perPage !== DEFAULT_PER_PAGE)).toBe(
      true,
    )
    expect(screen.getByRole('button', { name: /24 条\/页/ })).toBeInTheDocument()
  })

  it('页码越界时自动回退末页', async () => {
    fakeServer(makeMany(30))
    window.history.replaceState(null, '', '/?page=99&perPage=12')
    await renderDashboard()

    // 30 条、每页 12 → 末页是 3
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Page 3' })).toHaveAttribute(
        'aria-current',
        'page',
      ),
    )
    expect(checkboxOf(25)).toBeInTheDocument()
  })
})

describe('Dashboard 筛选', () => {
  it('筛选条件下发到列表请求', async () => {
    fakeServer(makeMany(30, { disabledIds: [1, 2, 3] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅已禁用' }))

    await waitFor(() =>
      expect(api.getCredentials).toHaveBeenLastCalledWith(
        expect.objectContaining({ disabled: true, page: 1 }),
      ),
    )
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(3))
  })

  it('改筛选条件时从第 2 页回到第 1 页', async () => {
    fakeServer(makeMany(30, { disabledIds: [1, 2, 3] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅已禁用' }))

    await waitFor(() =>
      expect(api.getCredentials).toHaveBeenLastCalledWith(
        expect.objectContaining({ disabled: true, page: 1 }),
      ),
    )
  })

  it('筛选后无结果时提示区分「无匹配」与「暂无凭据」', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅已禁用' }))

    expect(await screen.findByText('没有符合筛选条件的凭据')).toBeInTheDocument()
  })

  it('下拉可选值取自 facets 端点而非当前页数据', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: /全部等级/ }))
    const items = (await screen.findAllByRole('menuitem')).map((node) => node.textContent)

    expect(items).toContain('KIRO FREE')
    expect(items).toContain('KIRO PRO')
  })
})

describe('Dashboard URL 状态同步', () => {
  it('筛选与分页写入查询串', async () => {
    fakeServer(makeMany(60))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())

    await user.click(screen.getByRole('button', { name: /全部状态/ }))
    await user.click(await screen.findByRole('menuitem', { name: '仅未禁用' }))

    await waitFor(() => {
      const params = new URLSearchParams(window.location.search)
      expect(params.get('disabled')).toBe('false')
      expect(params.get('perPage')).toBe('12')
      // 改筛选回到第 1 页，第 1 页不写 page
      expect(params.get('page')).toBeNull()
    })
  })

  it('从查询串恢复筛选与分页', async () => {
    fakeServer(makeMany(60))
    window.history.replaceState(null, '', '/?disabled=false&page=2&perPage=24')
    await renderDashboard()

    await waitFor(() =>
      expect(api.getCredentials).toHaveBeenCalledWith(
        expect.objectContaining({ disabled: false, page: 2, perPage: 24 }),
      ),
    )
    await waitFor(() => expect(checkboxOf(25)).toBeInTheDocument())
  })
})

describe('Dashboard 全选与跨页选中', () => {
  it('全选只覆盖当前页，不扩展到筛选后全集', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: /全选本页/ }))

    expect(await screen.findByText('已选 12 条 / 当前页 12 条')).toBeInTheDocument()
  })

  it('翻页后已选不丢，计数跨页累计', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    // 第 1 页勾 3 条
    for (const id of [1, 2, 3]) await user.click(checkboxOf(id))
    expect(await screen.findByText('已选 3 条 / 当前页 12 条')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())

    // 翻页不清空
    expect(screen.getByText('已选 3 条 / 当前页 12 条')).toBeInTheDocument()

    // 第 2 页再勾 2 条 → 跨页累计 5
    for (const id of [13, 14]) await user.click(checkboxOf(id))
    expect(await screen.findByText('已选 5 条 / 当前页 12 条')).toBeInTheDocument()
  })

  it('部分选态可识别：按钮文案与 aria-pressed 三态', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    const button = () => screen.getByRole('button', { name: /全选本页|本页部分已选|取消本页全选/ })
    expect(button()).toHaveAttribute('aria-pressed', 'false')

    await user.click(checkboxOf(1))
    await waitFor(() => expect(button()).toHaveTextContent('本页部分已选，全选本页'))
    expect(button()).toHaveAttribute('aria-pressed', 'false')

    await user.click(button())
    await waitFor(() => expect(button()).toHaveTextContent('取消本页全选'))
    expect(button()).toHaveAttribute('aria-pressed', 'true')
  })

  it('已全选时再点只取消本页，其他页已选项不动', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    // 第 2 页勾 2 条
    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    // 回第 1 页全选，再点一次取消本页
    await user.click(screen.getByRole('button', { name: '上一页' }))
    await waitFor(() => expect(checkboxOf(1)).toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: /全选本页/ }))
    expect(await screen.findByText('已选 14 条 / 当前页 12 条')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: '取消本页全选' }))

    // 只剩第 2 页那 2 条
    expect(await screen.findByText('已选 2 条 / 当前页 12 条')).toBeInTheDocument()
  })
})

describe('Dashboard 批量操作的跨页判据', () => {
  it('批量删除：第 2 页选中的已禁用凭据翻回第 1 页后仍被识别，不计入已跳过', async () => {
    fakeServer(makeMany(30, { disabledIds: [13, 14] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    await user.click(screen.getByRole('button', { name: '上一页' }))
    await waitFor(() => expect(checkboxOf(1)).toBeInTheDocument())

    await user.click(screen.getByRole('button', { name: /批量删除/ }))

    // 确认文案里 2 个已禁用、0 个跳过
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('删除 2 个已禁用凭据'))
    expect(window.confirm).not.toHaveBeenCalledWith(expect.stringContaining('将跳过'))
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledTimes(2))
    expect(api.deleteCredential.mock.calls.map((c) => c[0])).toEqual([13, 14])
  })

  it('批量删除：已选凭据被他处删除时如实计为失败，不与已跳过混淆', async () => {
    const { store } = fakeServer(makeMany(30, { disabledIds: [13, 14] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    // #13 在别处已被删除：服务端不再有它，删除请求返回 404
    store.splice(
      store.findIndex((c) => c.id === 13),
      1,
    )
    api.deleteCredential.mockImplementation((id: number) => {
      const index = store.findIndex((c) => c.id === id)
      if (index < 0) return Promise.reject(new Error('凭据不存在'))
      store.splice(index, 1)
      return Promise.resolve({ success: true, message: '已删除' })
    })

    await user.click(screen.getByRole('button', { name: /批量删除/ }))

    // 两个都发了请求，#13 失败 #14 成功；失败与「已跳过」是两个独立计数
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledTimes(2))
    await waitFor(() =>
      expect(toast.warning).toHaveBeenCalledWith(
        expect.stringContaining('成功 1 个，失败 1 个'),
      ),
    )
    expect(toast.warning).not.toHaveBeenCalledWith(expect.stringContaining('已跳过'))
    // 选中集合被清空，消失的 #13 不残留
    await waitFor(() => expect(screen.queryByText(/已选 \d+ 条/)).not.toBeInTheDocument())
  })

  it('批量删除：混选时未禁用项计入已跳过而非误删', async () => {
    fakeServer(makeMany(30, { disabledIds: [13] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    await user.click(screen.getByRole('button', { name: /批量删除/ }))

    expect(window.confirm).toHaveBeenCalledWith(
      expect.stringContaining('将跳过 1 个未禁用凭据'),
    )
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledTimes(1))
    expect(api.deleteCredential).toHaveBeenCalledWith(13)
  })

  it('批量恢复异常：判据是失败次数，跨页选中仍生效', async () => {
    fakeServer(makeMany(30, { failedIds: [13] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    await user.click(screen.getByRole('button', { name: '上一页' }))
    await waitFor(() => expect(checkboxOf(1)).toBeInTheDocument())

    api.resetCredentialFailure.mockResolvedValue({ success: true, message: '已恢复' })
    await user.click(screen.getByRole('button', { name: /恢复异常/ }))

    await waitFor(() => expect(api.resetCredentialFailure).toHaveBeenCalledTimes(1))
    expect(api.resetCredentialFailure).toHaveBeenCalledWith(13)
  })

  it('批量刷新 Token：判据是未禁用，跨页选中仍生效', async () => {
    fakeServer(makeMany(30, { disabledIds: [14] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    for (const id of [13, 14]) await user.click(checkboxOf(id))

    await user.click(screen.getByRole('button', { name: '上一页' }))
    await waitFor(() => expect(checkboxOf(1)).toBeInTheDocument())

    api.forceRefreshToken.mockResolvedValue({ success: true, message: '已刷新' })
    await user.click(screen.getByRole('button', { name: /批量刷新 Token/ }))

    await waitFor(() => expect(api.forceRefreshToken).toHaveBeenCalledTimes(1))
    expect(api.forceRefreshToken).toHaveBeenCalledWith(13)
  })

  it('批量验活的作用范围是已选 id 集合，跨页累计', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(checkboxOf(1))
    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    await user.click(checkboxOf(13))

    await user.click(screen.getByRole('button', { name: /批量验活/ }))

    // 作用范围是两个已选 id，跨页累计
    const dialog = await screen.findByRole('dialog')
    expect(within(dialog).getByText('凭据 #1')).toBeInTheDocument()
    expect(within(dialog).getByText('凭据 #13')).toBeInTheDocument()
    // 总数取自已选集合大小，进度分母即为 2
    expect(within(dialog).getByText(/\/ 2$/)).toBeInTheDocument()

    // 传的是 id 而不是 [id, selection] 元组
    await waitFor(() => expect(api.getCredentialBalance).toHaveBeenCalledWith(1, true))
  })
})

describe('Dashboard 全量已禁用计数与清除', () => {
  it('当前页无已禁用但全量有时计数仍大于 0，按钮可用', async () => {
    // 已禁用项全在第 3 页，第 1 页一个都没有
    fakeServer(makeMany(30, { disabledIds: [25, 26, 27] }))
    await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    expect(pageCheckboxes()).toHaveLength(12)
    const button = await screen.findByRole('button', { name: /清除已禁用 \(3\)/ })
    expect(button).toBeEnabled()
  })

  it('全量无已禁用时按钮禁用且不带计数', async () => {
    fakeServer(makeMany(30))
    await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    const button = screen.getByRole('button', { name: '清除已禁用' })
    expect(button).toBeDisabled()
  })

  it('清除已禁用：确认文案与实际删除请求数都等于全量数', async () => {
    fakeServer(makeMany(30, { disabledIds: [25, 26, 27] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(await screen.findByRole('button', { name: /清除已禁用 \(3\)/ }))

    expect(window.confirm).toHaveBeenCalledWith(
      expect.stringContaining('清除所有 3 个已禁用凭据'),
    )
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledTimes(3))
    expect(api.deleteCredential.mock.calls.map((c) => c[0]).sort((a, b) => a - b)).toEqual([
      25, 26, 27,
    ])
    // 待删 id 来自全量查询，不是当前页响应
    expect(api.fetchAllDisabledIds).toHaveBeenCalledTimes(1)
  })

  it('清除已禁用：执行完毕后系统内不再存在已禁用凭据', async () => {
    // 已禁用项分散在第 1、2、3 页，验证后置条件而非只验请求数
    const { store } = fakeServer(makeMany(30, { disabledIds: [3, 15, 28] }))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    await user.click(await screen.findByRole('button', { name: /清除已禁用 \(3\)/ }))
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledTimes(3))

    // 服务端侧归零
    await waitFor(() => expect(store.filter((c) => c.disabled)).toHaveLength(0))
    expect(store).toHaveLength(27)
    // 入口计数与作用范围同源，归零后按钮转为禁用且不带计数
    await waitFor(() => {
      const button = screen.getByRole('button', { name: /清除已禁用/ })
      expect(button).toBeDisabled()
      expect(button).toHaveAccessibleName('清除已禁用')
    })
  })
})

describe('Dashboard 实时余额缓存清理', () => {
  it('翻页往返后已查到的实时值仍在', async () => {
    fakeServer(makeMany(30))
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    // 批量查询当前页余额，得到实时值
    await user.click(screen.getByRole('button', { name: /批量余额\/订阅/ }))
    await waitFor(() => expect(screen.getAllByText(/90\.00 \/ 100\.00/).length).toBe(12))

    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: '上一页' }))
    await waitFor(() => expect(checkboxOf(1)).toBeInTheDocument())

    // 判据是「已从系统删除」而非「不在当前页」，翻页不该清掉
    expect(screen.getAllByText(/90\.00 \/ 100\.00/).length).toBe(12)
  })

  it('删除凭据后对应实时值与选中项一并移除', async () => {
    // #13 先保持启用才能被批量余额覆盖：只有它真拿到过实时值，「移除」才有可验证的对象。
    // 批量删除只删已禁用项，所以查完余额再从 UI 把它禁用。
    const { store } = fakeServer(makeMany(13))
    api.setCredentialDisabled.mockImplementation((id: number, disabled: boolean) => {
      const target = store.find((c) => c.id === id)
      if (target) target.disabled = disabled
      return Promise.resolve({ success: true, message: '已更新' })
    })
    const { user } = await renderDashboard()
    await waitFor(() => expect(pageCheckboxes()).toHaveLength(12))

    // 第 2 页只有 #13，批量查询后它拿到实时值
    await user.click(screen.getByRole('button', { name: '下一页' }))
    await waitFor(() => expect(checkboxOf(13)).toBeInTheDocument())
    await user.click(screen.getByRole('button', { name: /批量余额\/订阅/ }))
    await waitFor(() => expect(screen.getAllByText(/90\.00 \/ 100\.00/)).toHaveLength(1))

    // 禁用它以满足删除前置条件，实时值此时仍在。页面上还有「定时刷新」开关，
    // 取当前勾选态为 true 的那个（卡片的启用开关）
    const enableSwitch = screen
      .getAllByRole('switch')
      .find((el) => el.getAttribute('aria-checked') === 'true')!
    await user.click(enableSwitch)
    await waitFor(() => expect(screen.getByText('已禁用')).toBeInTheDocument())
    expect(screen.getAllByText(/90\.00 \/ 100\.00/)).toHaveLength(1)

    await user.click(checkboxOf(13))
    await user.click(screen.getByRole('button', { name: /批量删除/ }))
    await waitFor(() => expect(api.deleteCredential).toHaveBeenCalledWith(13))

    // 实时值随凭据一并移除，不残留在界面上
    await waitFor(() => expect(screen.queryByText(/90\.00 \/ 100\.00/)).not.toBeInTheDocument())
    // 选中集合同步清空，已选计数区消失
    await waitFor(() => expect(screen.queryByText(/^已选 /)).not.toBeInTheDocument())
  })
})
