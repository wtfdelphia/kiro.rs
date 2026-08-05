import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import type { ModelsRefreshAllResponse } from '@/types/api'

interface ModelsRefreshResultDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  result: ModelsRefreshAllResponse | null
}

export function ModelsRefreshResultDialog({
  open,
  onOpenChange,
  result,
}: ModelsRefreshResultDialogProps) {
  if (!result) return null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>全量模型刷新结果</DialogTitle>
          <DialogDescription>
            成功 {result.refreshed}，失败 {result.failed}，全局目录 {result.globalCount} 个模型
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-72 overflow-y-auto space-y-2">
          {result.errors.length === 0 ? (
            <div className="text-sm text-muted-foreground text-center py-4">
              无失败项
            </div>
          ) : (
            result.errors.map((item) => (
              <div
                key={`${item.credentialId}-${item.error.slice(0, 24)}`}
                className="text-sm border rounded-md p-2 space-y-1"
              >
                <div className="font-medium">凭据 #{item.credentialId}</div>
                <div className="text-red-500 break-words whitespace-pre-wrap">
                  {item.error}
                </div>
              </div>
            ))
          )}
        </div>

        <DialogFooter>
          <Button onClick={() => onOpenChange(false)}>关闭</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
