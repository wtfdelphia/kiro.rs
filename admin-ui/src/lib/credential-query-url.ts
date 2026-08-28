/** 凭据列表查询状态与 URL 查询串的双向映射（可单测，不依赖 React） */

import type { CredentialsQuery } from '@/types/api'

/** 七个筛选维度，不含分页 */
export type CredentialFilters = Omit<CredentialsQuery, 'page' | 'perPage'>

const TEXT_KEYS = ['subscriptionTitle', 'authMethod', 'email', 'id'] as const
const BOOL_KEYS = ['disabled', 'hasProfileArn'] as const
const NUM_KEYS = ['priorityMin', 'priorityMax'] as const

/**
 * 把查询状态写成查询串（不含 `?`）。
 *
 * 缺省与空串一律不写入：URL 里出现 `email=` 会在读回来时变成「筛选空 email」，
 * 与「不按 email 筛选」不是一回事。
 */
export function queryToSearchParams(
  filters: CredentialFilters,
  page: number,
  perPage: number,
): string {
  const params = new URLSearchParams()
  for (const key of TEXT_KEYS) {
    const value = filters[key]
    if (value !== undefined && value !== '') params.set(key, value)
  }
  for (const key of BOOL_KEYS) {
    const value = filters[key]
    if (value !== undefined) params.set(key, String(value))
  }
  for (const key of NUM_KEYS) {
    const value = filters[key]
    if (value !== undefined) params.set(key, String(value))
  }
  if (page > 1) params.set('page', String(page))
  params.set('perPage', String(perPage))
  return params.toString()
}

/** 从查询串恢复筛选维度，无法解析的取值按缺省处理 */
export function searchParamsToFilters(search: string): CredentialFilters {
  const params = new URLSearchParams(search)
  const filters: CredentialFilters = {}
  for (const key of TEXT_KEYS) {
    const value = params.get(key)
    if (value) filters[key] = value
  }
  for (const key of BOOL_KEYS) {
    const value = params.get(key)
    if (value === 'true') filters[key] = true
    else if (value === 'false') filters[key] = false
  }
  for (const key of NUM_KEYS) {
    const value = Number(params.get(key))
    if (params.get(key) !== null && Number.isInteger(value) && value >= 0) {
      filters[key] = value
    }
  }
  return filters
}

/** 从查询串恢复页码，非正整数回落 1 */
export function searchParamsToPage(search: string): number {
  const raw = new URLSearchParams(search).get('page')
  const value = Number(raw)
  return Number.isInteger(value) && value >= 1 ? value : 1
}

/** 从查询串恢复每页条数，缺省或非法时返回 null 交由调用方回落本地存储 */
export function searchParamsToPerPage(search: string): number | null {
  const raw = new URLSearchParams(search).get('perPage')
  const value = Number(raw)
  return Number.isInteger(value) && value >= 1 ? value : null
}
