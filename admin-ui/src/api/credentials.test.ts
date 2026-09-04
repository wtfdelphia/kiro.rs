import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { CredentialsStatusResponse, PageInfo } from '@/types/api'

const get = vi.hoisted(() => vi.fn())

vi.mock('axios', () => ({
  default: {
    create: () => ({
      get,
      post: vi.fn(),
      put: vi.fn(),
      delete: vi.fn(),
      interceptors: { request: { use: vi.fn() } },
    }),
  },
}))

const { fetchAllDisabledIds, getCredentials } = await import('./credentials')

/** 取最近一次请求实际下发的查询参数 */
function lastParams(): Record<string, unknown> {
  const calls = get.mock.calls
  return calls[calls.length - 1]?.[1]?.params ?? {}
}

function pageInfo(overrides: Partial<PageInfo> = {}): PageInfo {
  return {
    page: 1,
    perPage: 100,
    filteredTotal: 0,
    totalPages: 1,
    hasPrev: false,
    hasNext: false,
    ...overrides,
  }
}

function response(ids: number[], info: Partial<PageInfo> = {}): { data: CredentialsStatusResponse } {
  return {
    data: {
      total: ids.length,
      available: 0,
      currentId: 0,
      credentials: ids.map((id) => ({ id })) as CredentialsStatusResponse['credentials'],
      pageInfo: pageInfo(info),
    },
  }
}

describe('getCredentials', () => {
  beforeEach(() => {
    get.mockReset()
    get.mockResolvedValue(response([]))
  })

  it('把查询对象下发为查询参数', async () => {
    await getCredentials({ authMethod: 'idc', disabled: false, page: 2, perPage: 24 })
    expect(lastParams()).toEqual({ authMethod: 'idc', disabled: false, page: 2, perPage: 24 })
  })

  it('空串与 undefined 不下发', async () => {
    await getCredentials({ email: '', subscriptionTitle: undefined, id: '12' })
    expect(lastParams()).toEqual({ id: '12' })
  })

  it('不传参数时不带任何查询串', async () => {
    await getCredentials()
    expect(lastParams()).toEqual({})
  })

  it('disabled=false 是有效筛选值，不能当空值丢掉', async () => {
    await getCredentials({ disabled: false })
    expect(lastParams()).toEqual({ disabled: false })
  })
})

describe('fetchAllDisabledIds', () => {
  beforeEach(() => {
    get.mockReset()
  })

  it('单页取回', async () => {
    get.mockResolvedValueOnce(response([1, 2, 3], { filteredTotal: 3, hasNext: false }))
    expect(await fetchAllDisabledIds()).toEqual([1, 2, 3])
    expect(get).toHaveBeenCalledTimes(1)
    expect(lastParams()).toEqual({ disabled: true, page: 1, perPage: 100 })
  })

  it('多页续取直到 hasNext 为 false', async () => {
    get
      .mockResolvedValueOnce(response([1, 2], { page: 1, totalPages: 3, hasNext: true }))
      .mockResolvedValueOnce(response([3, 4], { page: 2, totalPages: 3, hasNext: true }))
      .mockResolvedValueOnce(response([5], { page: 3, totalPages: 3, hasNext: false }))

    expect(await fetchAllDisabledIds()).toEqual([1, 2, 3, 4, 5])
    expect(get).toHaveBeenCalledTimes(3)
    expect(get.mock.calls.map((c) => c[1].params.page)).toEqual([1, 2, 3])
  })
})
