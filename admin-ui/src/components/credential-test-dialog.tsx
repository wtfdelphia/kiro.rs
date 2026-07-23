import { useState } from 'react'
import { toast } from 'sonner'
import { Loader2, FlaskConical } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { testCredential } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { TestCredentialResponse } from '@/types/api'

interface CredentialTestDialogProps {
  credentialId: number | null
  open: boolean
  onOpenChange: (open: boolean) => void
  disabled?: boolean
}

export function CredentialTestDialog({
  credentialId,
  open,
  onOpenChange,
  disabled = false,
}: CredentialTestDialogProps) {
  const [model, setModel] = useState('')
  const [pending, setPending] = useState(false)
  const [result, setResult] = useState<TestCredentialResponse | null>(null)
  const [error, setError] = useState<string | null>(null)

  const handleOpenChange = (next: boolean) => {
    if (!next) {
      setModel('')
      setResult(null)
      setError(null)
    }
    onOpenChange(next)
  }

  const handleSubmit = async () => {
    if (credentialId == null || pending || disabled) return
    setPending(true)
    setResult(null)
    setError(null)
    try {
      const resp = await testCredential(
        credentialId,
        model.trim() ? model.trim() : undefined
      )
      setResult(resp)
      toast.success(
        `测试成功：${resp.model}（${resp.latencyMs} ms）`
      )
    } catch (e) {
      const msg = extractErrorMessage(e)
      setError(msg)
      toast.error(`测试失败: ${msg}`)
    } finally {
      setPending(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>凭据 #{credentialId} 推理测试</DialogTitle>
          <DialogDescription>
            对上游发起最小真实推理探测（非流式）。留空 model 时使用服务端默认。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <label className="text-sm text-muted-foreground">模型（可选）</label>
            <Input
              placeholder="例如 claude-sonnet-4-6 或 claude-sonnet-5"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              disabled={pending || disabled}
            />
          </div>

          {error && (
            <div className="text-sm text-red-500 break-words border border-red-500/30 rounded-md p-2">
              {error}
            </div>
          )}

          {result && (
            <div className="space-y-2 text-sm rounded-md border p-3">
              <div>
                <span className="text-muted-foreground">模型：</span>
                <span className="font-mono">{result.model}</span>
              </div>
              <div>
                <span className="text-muted-foreground">延迟：</span>
                {result.latencyMs} ms
              </div>
              <div>
                <span className="text-muted-foreground">回复：</span>
                <div className="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap break-words">
                  {result.reply || '（空）'}
                </div>
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => handleOpenChange(false)}
            disabled={pending}
          >
            关闭
          </Button>
          <Button onClick={handleSubmit} disabled={pending || disabled || credentialId == null}>
            {pending ? (
              <>
                <Loader2 className="h-4 w-4 mr-1 animate-spin" />
                测试中...
              </>
            ) : (
              <>
                <FlaskConical className="h-4 w-4 mr-1" />
                开始测试
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
