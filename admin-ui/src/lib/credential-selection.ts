/** 凭据选中状态的数据结构与派生逻辑（可单测，不依赖 React） */

import type { CredentialStatusItem } from '@/types/api'

/**
 * 勾选那一刻从当前页快照下来的凭据事实。
 *
 * 服务端分页后 `data.credentials` 只有当前页，靠 id 回列表反查会把
 * 跨页已选凭据误判成「不符合条件」，所以判据随选中一起存。
 */
export interface CredentialSelection {
  id: number
  disabled: boolean
  /** 批量恢复异常要判失败次数，不只看禁用状态 */
  failureCount: number
}

export function toSelection(credential: CredentialStatusItem): CredentialSelection {
  return {
    id: credential.id,
    disabled: credential.disabled,
    failureCount: credential.failureCount,
  }
}

/** 当前页的整体勾选态 */
export type PageSelectionState = 'none' | 'partial' | 'all'

export function pageSelectionState(
  pageIds: number[],
  selected: ReadonlyMap<number, CredentialSelection>,
): PageSelectionState {
  if (pageIds.length === 0) return 'none'
  const hit = pageIds.filter((id) => selected.has(id)).length
  if (hit === 0) return 'none'
  return hit === pageIds.length ? 'all' : 'partial'
}

/**
 * 按当前页勾选态切换选中集合。
 *
 * 作用域只限当前页：未全选时补齐本页，已全选时只摘掉本页，
 * 其他页已选条目一概不动，也不隐式扩展到筛选后的全集。
 */
export function toggleCurrentPage(
  page: CredentialStatusItem[],
  selected: ReadonlyMap<number, CredentialSelection>,
): Map<number, CredentialSelection> {
  const next = new Map(selected)
  const state = pageSelectionState(
    page.map((credential) => credential.id),
    selected,
  )
  if (state === 'all') {
    page.forEach((credential) => next.delete(credential.id))
  } else {
    page.forEach((credential) => next.set(credential.id, toSelection(credential)))
  }
  return next
}
