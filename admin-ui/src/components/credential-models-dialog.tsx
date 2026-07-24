import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { Loader2, RefreshCw, FlaskConical } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  getCredentialModels,
  refreshCredentialModels,
} from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialModelsResponse } from '@/types/api'

interface CredentialModelsDialogProps {
  credentialId: number | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onTestModel?: (modelId: string) => void
  onModelsChanged?: () => void
}

export function CredentialModelsDialog({
  credentialId,
  open,
  onOpenChange,
  onTestModel,
  onModelsChanged,
}: CredentialModelsDialogProps) {
  const [loading, setLoading] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [data, setData] = useState<CredentialModelsResponse | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = async (live = false) => {
    if (credentialId == null) return
    setLoading(true)
    setError(null)
    try {
      const resp = await getCredentialModels(credentialId, live)
      setData(resp)
    } catch (e) {
      setData(null)
      setError(extractErrorMessage(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (open && credentialId != null) {
      void load(false)
    }
    if (!open) {
      setData(null)
      setError(null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, credentialId])

  const handleRefresh = async () => {
    if (credentialId == null || refreshing) return
    setRefreshing(true)
    try {
      const resp = await refreshCredentialModels(credentialId)
      toast.success(`凭据 #${credentialId} 模型已刷新：${resp.count} 个`)
      setData({
        success: true,
        models: resp.models,
        updatedAt: resp.updatedAt,
        lastError: null,
      })
      setError(null)
      onModelsChanged?.()
    } catch (e) {
      toast.error(`刷新模型失败: ${extractErrorMessage(e)}`)
    } finally {
      setRefreshing(false)
    }
  }

  const handleLive = async () => {
    await load(true)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>凭据 #{credentialId} 模型目录</DialogTitle>
          <DialogDescription>
            显示该凭据缓存的上游可用模型；可刷新、实时拉取，或用某模型发起测试。
          </DialogDescription>
        </DialogHeader>

        {loading && (
          <div className="flex items-center justify-center py-8 text-muted-foreground">
            <Loader2 className="h-6 w-6 animate-spin mr-2" />
            加载中...
          </div>
        )}

        {!loading && error && (
          <div className="py-4 text-sm text-red-500 break-words">{error}</div>
        )}

        {!loading && data && (
          <div className="space-y-3">
            <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
              {data.updatedAt && <span>更新于：{data.updatedAt}</span>}
              <Badge variant="secondary">{data.models.length} 个模型</Badge>
            </div>
            {data.lastError && (
              <div className="text-sm text-amber-700 dark:text-amber-400 break-words border border-amber-500/30 rounded-md p-2">
                lastError: {data.lastError}
              </div>
            )}
            <div className="max-h-64 overflow-y-auto rounded-md border p-2 space-y-1">
              {data.models.length === 0 ? (
                <div className="text-sm text-muted-foreground py-4 text-center">
                  暂无缓存模型（可尝试刷新或实时拉取）
                </div>
              ) : (
                data.models.map((m) => (
                  <div
                    key={m}
                    className="flex items-center justify-between gap-2 px-2 py-1 rounded hover:bg-muted"
                  >
                    <span className="font-mono text-xs break-all">{m}</span>
                    {onTestModel && (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 shrink-0"
                        onClick={() => onTestModel(m)}
                        title="用此模型测试"
                      >
                        <FlaskConical className="h-3.5 w-3.5 mr-1" />
                        测试
                      </Button>
                    )}
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        <DialogFooter className="flex-wrap gap-2 sm:justify-between">
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleLive}
              disabled={loading || refreshing || credentialId == null}
            >
              实时拉取
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handleRefresh}
              disabled={loading || refreshing || credentialId == null}
            >
              <RefreshCw className={`h-4 w-4 mr-1 ${refreshing ? 'animate-spin' : ''}`} />
              刷新模型
            </Button>
          </div>
          <Button variant="default" size="sm" onClick={() => onOpenChange(false)}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
