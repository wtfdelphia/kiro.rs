import { describe, expect, it } from 'vitest'
import { SUBSCRIPTION_TITLE_UNKNOWN } from '@/types/api'
import {
  queryToSearchParams,
  searchParamsToFilters,
  searchParamsToPage,
  searchParamsToPerPage,
  type CredentialFilters,
} from './credential-query-url'

const full: CredentialFilters = {
  subscriptionTitle: 'KIRO PRO+',
  authMethod: 'social',
  email: 'a@b.com',
  id: '12',
  disabled: false,
  hasProfileArn: true,
  priorityMin: 0,
  priorityMax: 5,
}

describe('查询串往返', () => {
  it('七个维度加分页写入后能原样读回', () => {
    const search = queryToSearchParams(full, 3, 24)

    expect(searchParamsToFilters(search)).toEqual(full)
    expect(searchParamsToPage(search)).toBe(3)
    expect(searchParamsToPerPage(search)).toBe(24)
  })

  it('哨兵值不被特殊处理，原样往返', () => {
    const search = queryToSearchParams({ subscriptionTitle: SUBSCRIPTION_TITLE_UNKNOWN }, 1, 12)
    expect(searchParamsToFilters(search).subscriptionTitle).toBe(SUBSCRIPTION_TITLE_UNKNOWN)
  })

  it('空筛选往返后仍为空对象', () => {
    const search = queryToSearchParams({}, 1, 12)
    expect(searchParamsToFilters(search)).toEqual({})
  })
})

describe('queryToSearchParams', () => {
  it('缺省与空串都不写入，避免读回时变成「筛选空值」', () => {
    const search = queryToSearchParams({ email: '', id: undefined }, 1, 12)
    expect(search).not.toContain('email')
    expect(search).not.toContain('id=')
  })

  it('第 1 页不写 page，每页条数始终写入', () => {
    expect(queryToSearchParams({}, 1, 12)).toBe('perPage=12')
    expect(queryToSearchParams({}, 2, 12)).toBe('page=2&perPage=12')
  })

  it('false 与 0 属于有效取值，必须写入', () => {
    const search = queryToSearchParams({ disabled: false, priorityMin: 0 }, 1, 12)
    expect(search).toContain('disabled=false')
    expect(search).toContain('priorityMin=0')
  })
})

describe('searchParamsToFilters', () => {
  it('布尔维度只认 true/false，其他取值按缺省处理', () => {
    expect(searchParamsToFilters('disabled=yes')).toEqual({})
    expect(searchParamsToFilters('disabled=true')).toEqual({ disabled: true })
    expect(searchParamsToFilters('disabled=false')).toEqual({ disabled: false })
  })

  it('优先级只认非负整数', () => {
    expect(searchParamsToFilters('priorityMin=-1&priorityMax=abc')).toEqual({})
    expect(searchParamsToFilters('priorityMin=0')).toEqual({ priorityMin: 0 })
  })
})

describe('页码与每页条数解析', () => {
  it('页码非正整数时回落第 1 页', () => {
    expect(searchParamsToPage('')).toBe(1)
    expect(searchParamsToPage('page=0')).toBe(1)
    expect(searchParamsToPage('page=-2')).toBe(1)
    expect(searchParamsToPage('page=1.5')).toBe(1)
  })

  it('每页条数缺省或非法时返回 null，交由调用方回落本地存储', () => {
    expect(searchParamsToPerPage('')).toBeNull()
    expect(searchParamsToPerPage('perPage=0')).toBeNull()
    expect(searchParamsToPerPage('perPage=abc')).toBeNull()
    expect(searchParamsToPerPage('perPage=48')).toBe(48)
  })
})
