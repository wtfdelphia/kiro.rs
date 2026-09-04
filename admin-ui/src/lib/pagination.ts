/** 分页控件的页码折叠逻辑（可单测，不依赖 React） */

export type PageItem = number | 'ellipsis'

/**
 * 生成页码条内容，仿 Primer Pagination 的双参数折叠模型。
 *
 * - `marginPageCount`：首尾各保留的页码数
 * - `surroundingPageCount`：当前页两侧各保留的页码数
 *
 * 折叠区间只剩一页时直接渲染该页码：省略号占位跟页码一样宽，
 * 用它替代单个页码既不省空间又少一个可点目标。
 */
export function buildPageItems(
  current: number,
  totalPages: number,
  marginPageCount = 1,
  surroundingPageCount = 2,
): PageItem[] {
  if (totalPages <= 0) return []

  const keep = new Set<number>()
  for (let i = 1; i <= Math.min(marginPageCount, totalPages); i++) keep.add(i)
  for (let i = Math.max(1, totalPages - marginPageCount + 1); i <= totalPages; i++) keep.add(i)
  for (
    let i = Math.max(1, current - surroundingPageCount);
    i <= Math.min(totalPages, current + surroundingPageCount);
    i++
  ) {
    keep.add(i)
  }

  const items: PageItem[] = []
  let prev = 0
  for (const page of [...keep].sort((a, b) => a - b)) {
    const gap = page - prev - 1
    if (gap === 1) {
      // 只隔一页，补上真实页码而不是省略号
      items.push(prev + 1)
    } else if (gap > 1) {
      items.push('ellipsis')
    }
    items.push(page)
    prev = page
  }
  return items
}
