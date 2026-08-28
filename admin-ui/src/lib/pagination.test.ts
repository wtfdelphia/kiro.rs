import { describe, expect, it } from 'vitest'
import { buildPageItems } from './pagination'

describe('buildPageItems', () => {
  it('页数少时不出现省略号', () => {
    expect(buildPageItems(1, 5)).toEqual([1, 2, 3, 4, 5])
    expect(buildPageItems(3, 5)).toEqual([1, 2, 3, 4, 5])
  })

  it('多页时两侧折叠', () => {
    expect(buildPageItems(10, 20)).toEqual([1, 'ellipsis', 8, 9, 10, 11, 12, 'ellipsis', 20])
  })

  it('折叠区间只剩一页时渲染该页码', () => {
    // 保留 1、4-8、12：1 与 4 之间隔了 2、3 两页 → 省略号；
    // 缩到 surrounding=2、current=4 时 1 与 2 相邻，无需省略号
    expect(buildPageItems(4, 8)).toEqual([1, 2, 3, 4, 5, 6, 7, 8])
    // 1 与 3 之间只隔第 2 页，应补 2 而不是省略号
    expect(buildPageItems(5, 20, 1, 2)).toEqual([1, 2, 3, 4, 5, 6, 7, 'ellipsis', 20])
  })

  it('当前页在首尾时另一侧折叠', () => {
    expect(buildPageItems(1, 20)).toEqual([1, 2, 3, 'ellipsis', 20])
    expect(buildPageItems(20, 20)).toEqual([1, 'ellipsis', 18, 19, 20])
  })

  it('零页返回空数组', () => {
    expect(buildPageItems(1, 0)).toEqual([])
  })
})
