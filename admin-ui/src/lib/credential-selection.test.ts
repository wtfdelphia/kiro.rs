import { describe, expect, it } from 'vitest'
import { makeCredential } from '@/test/render'
import {
  pageSelectionState,
  toSelection,
  toggleCurrentPage,
  type CredentialSelection,
} from './credential-selection'

/** 两页各 3 条，id 连续，便于断言「作用域只限当前页」 */
const page1 = [1, 2, 3].map((id) => makeCredential({ id }))
const page2 = [4, 5, 6].map((id) => makeCredential({ id }))

function selectionOf(credentials: ReturnType<typeof makeCredential>[]) {
  return new Map<number, CredentialSelection>(credentials.map((c) => [c.id, toSelection(c)]))
}

describe('toSelection', () => {
  it('带齐批量操作需要的三个判据', () => {
    const selection = toSelection(makeCredential({ id: 7, disabled: true, failureCount: 3 }))
    expect(selection).toEqual({ id: 7, disabled: true, failureCount: 3 })
  })
})

describe('pageSelectionState', () => {
  it('空页视为未选', () => {
    expect(pageSelectionState([], selectionOf(page1))).toBe('none')
  })

  it('本页一条未选时为 none', () => {
    expect(pageSelectionState([1, 2, 3], selectionOf(page2))).toBe('none')
  })

  it('本页部分已选时为 partial', () => {
    expect(pageSelectionState([1, 2, 3], selectionOf([page1[0]]))).toBe('partial')
  })

  it('本页全部已选时为 all，跨页多选不影响判定', () => {
    const selected = selectionOf([...page1, ...page2])
    expect(pageSelectionState([1, 2, 3], selected)).toBe('all')
  })
})

describe('toggleCurrentPage', () => {
  it('全选只覆盖当前页', () => {
    const next = toggleCurrentPage(page1, new Map())
    expect(Array.from(next.keys())).toEqual([1, 2, 3])
    expect(next.has(4)).toBe(false)
  })

  it('翻页后再全选不丢已选项', () => {
    const afterPage1 = toggleCurrentPage(page1, new Map())
    const afterPage2 = toggleCurrentPage(page2, afterPage1)
    expect(Array.from(afterPage2.keys())).toEqual([1, 2, 3, 4, 5, 6])
  })

  it('再次点击只取消本页，其他页已选项不动', () => {
    const all = selectionOf([...page1, ...page2])
    const next = toggleCurrentPage(page1, all)
    expect(Array.from(next.keys())).toEqual([4, 5, 6])
  })

  it('部分选态下补齐本页而不是清空', () => {
    const next = toggleCurrentPage(page1, selectionOf([page1[1]]))
    expect(Array.from(next.keys()).sort()).toEqual([1, 2, 3])
  })

  it('不改动传入的集合', () => {
    const before = selectionOf([page1[0]])
    toggleCurrentPage(page1, before)
    expect(Array.from(before.keys())).toEqual([1])
  })

  it('勾选的条目带齐属性快照', () => {
    const page = [makeCredential({ id: 9, disabled: true, failureCount: 2 })]
    const next = toggleCurrentPage(page, new Map())
    expect(next.get(9)).toEqual({ id: 9, disabled: true, failureCount: 2 })
  })
})
