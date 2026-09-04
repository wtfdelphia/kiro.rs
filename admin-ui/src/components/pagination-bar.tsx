import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { buildPageItems } from '@/lib/pagination'
import { PER_PAGE_OPTIONS } from '@/lib/storage'
import type { PageInfo } from '@/types/api'

interface PaginationBarProps {
  pageInfo: PageInfo
  onPageChange: (page: number) => void
  onPerPageChange: (perPage: number) => void
}

/**
 * 分页控件：上下页 + 折叠页码条 + 每页条数选择器。
 *
 * 边界按钮渲染为禁用态而不是移除，否则首末页时控件整体宽度会跳一下。
 */
export function PaginationBar({ pageInfo, onPageChange, onPerPageChange }: PaginationBarProps) {
  const { page, perPage, totalPages, filteredTotal, hasPrev, hasNext } = pageInfo
  const items = buildPageItems(page, totalPages)

  return (
    <nav aria-label="分页导航" className="flex flex-wrap justify-center items-center gap-2 mt-6">
      <Button variant="outline" size="sm" disabled={!hasPrev} onClick={() => onPageChange(page - 1)}>
        上一页
      </Button>

      {items.map((item, index) =>
        item === 'ellipsis' ? (
          // 省略号只是视觉占位：不可聚焦，也不进读屏
          <span
            key={`ellipsis-${index}`}
            aria-hidden="true"
            className="px-2 text-sm text-muted-foreground select-none"
          >
            …
          </span>
        ) : (
          <Button
            key={item}
            size="sm"
            variant={item === page ? 'default' : 'outline'}
            aria-label={`Page ${item}`}
            aria-current={item === page ? 'page' : undefined}
            onClick={() => onPageChange(item)}
            className="min-w-9 px-2"
          >
            {item}
          </Button>
        )
      )}

      <Button variant="outline" size="sm" disabled={!hasNext} onClick={() => onPageChange(page + 1)}>
        下一页
      </Button>

      <Select
        value={String(perPage)}
        onValueChange={(value) => onPerPageChange(Number(value))}
        options={PER_PAGE_OPTIONS.map((n) => ({ value: String(n), label: `${n} 条/页` }))}
        triggerClassName="h-9 w-24 px-2 text-sm"
      />

      <span aria-live="polite" className="text-sm text-muted-foreground">
        第 {page} / {totalPages} 页（共 {filteredTotal} 个凭据）
      </span>
    </nav>
  )
}
