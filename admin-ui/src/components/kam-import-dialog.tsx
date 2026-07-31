import { useState, useMemo } from 'react'
import { toast } from 'sonner'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials } from '@/hooks/use-credentials'
import { importKamDocument, type KamPreviewItem } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'

interface KamImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}
interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'verified_warn' | 'duplicate' | 'failed' | 'skipped'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  hasProfileArn?: boolean
  provider?: string | null
  profileWarning?: string
}

// 仅做 JSON 语法检查。容器判别与认证分类一律交给服务端：
// 客户端再实现一套规则会让同一份文件在不同入口得到不同结果。
export function parseJsonDocument(raw: string): unknown {
  const trimmed = raw.trim()
  if (!trimmed) throw new Error('请输入 KAM 导出的 JSON 内容')
  return JSON.parse(trimmed)
}

// 从服务端预检结果生成人读摘要（不含任何密钥材料）
export function describePreviewItem(item: KamPreviewItem): string {
  const parts: string[] = []
  parts.push(item.authMethod || '类型未识别')
  if (item.provider) parts.push(item.provider)
  const fields: string[] = []
  if (item.hasClientId) fields.push('clientId')
  if (item.hasClientSecret) fields.push('clientSecret')
  if (item.hasTokenEndpoint) fields.push('tokenEndpoint')
  if (item.hasIssuerUrl) fields.push('issuerUrl')
  if (item.hasScopes) fields.push('scopes')
  if (item.hasProfileArn) fields.push('profileArn')
  if (fields.length > 0) parts.push(fields.join('/'))
  if (item.disabled) parts.push('导入后禁用')
  return parts.join(' · ')
}

// 容器形态的中文说明
export function describeContainer(container: string): string {
  switch (container) {
    case 'FlatArray': return '平铺数组'
    case 'FlatObject': return '平铺单对象'
    case 'Wrapper': return '{ version, accounts } 包装'
    case 'LegacyNested': return '旧版 credentials 嵌套'
    default: return container
  }
}

export function KamImportDialog({ open, onOpenChange }: KamImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])
  const [preview, setPreview] = useState<KamPreviewItem[]>([])
  const [container, setContainer] = useState<string>('')

  const { refetch } = useCredentials()

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
    setPreview([])
    setContainer('')
  }

  const handleImport = async () => {
    let document: unknown
    try {
      document = parseJsonDocument(jsonInput)
    } catch (error) {
      toast.error('JSON 格式错误: ' + extractErrorMessage(error))
      return
    }

    try {
      setImporting(true)
      setCurrentProcessing('服务端解析与预检...')
      setResults([])
      setProgress({ current: 0, total: 0 })

      // 服务端完成容器判别、认证分类与逐条入库
      const resp = await importKamDocument({
        document,
        options: { onConflict: 'upsert', stopOnError: false, fetchBalance: true, concurrency: 1 },
      })

      setPreview(resp.preview)
      setContainer(resp.container)
      setProgress({ current: resp.preview.length, total: resp.preview.length })
      setCurrentProcessing('')

      // 逐条渲染：预检失败与入库结果都要可见，不静默丢弃任何记录
      const byIndex = new Map(resp.results.map(r => [r.index, r]))
      const rendered: VerificationResult[] = resp.preview.map(item => {
        if (!item.valid) {
          return {
            index: item.index + 1,
            status: 'failed' as const,
            error: item.error || '预检未通过',
            email: item.email || item.nickname,
            provider: item.provider ?? null,
          }
        }

        const r = byIndex.get(item.index)
        if (!r) {
          return {
            index: item.index + 1,
            status: 'skipped' as const,
            error: '未返回入库结果',
            email: item.email || item.nickname,
            provider: item.provider ?? null,
          }
        }

        const hasWarn = !!r.warning
        let status: VerificationResult['status'] = 'failed'
        if (r.status === 'duplicate') status = 'duplicate'
        else if (r.status === 'updated' || r.status === 'created') {
          status = hasWarn ? 'verified_warn' : 'verified'
        } else if (r.credentialId) {
          status = hasWarn ? 'verified_warn' : 'verified'
        }

        return {
          index: item.index + 1,
          status,
          error: r.error,
          email: r.email || item.email || item.nickname,
          credentialId: r.credentialId,
          usage: r.balance ? r.balance.currentUsage + '/' + r.balance.usageLimit : undefined,
          profileWarning: r.warning || undefined,
          hasProfileArn: r.warning ? false : r.credentialId ? true : undefined,
          provider: item.provider ?? null,
        }
      })
      setResults(rendered)

      await refetch()

      const successCount = rendered.filter(
        r => r.status === 'verified' || r.status === 'verified_warn'
      ).length
      const duplicateCount = rendered.filter(r => r.status === 'duplicate').length
      const failCount = rendered.filter(r => r.status === 'failed').length

      const summary =
        '成功 ' + successCount + '，重复 ' + duplicateCount + '，失败 ' + failCount
      if (successCount > 0) {
        toast.success('KAM 导入完成（' + describeContainer(resp.container) + '）：' + summary)
      } else {
        toast.error('KAM 导入未成功：' + summary)
      }
    } catch (error) {
      toast.error('导入失败: ' + extractErrorMessage(error))
    } finally {
      setImporting(false)
      setCurrentProcessing('')
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-gray-300" />
      case 'checking':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />
      case 'verified':
        return <CheckCircle2 className="w-5 h-5 text-green-500" />
      case 'verified_warn':
        return <AlertCircle className="w-5 h-5 text-amber-500" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-yellow-500" />
      case 'skipped':
        return <AlertCircle className="w-5 h-5 text-gray-400" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-red-500" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending': return '等待中'
      case 'checking': return '检查重复...'
      case 'verifying': return '验活中...'
      case 'verified': return '验活成功'
      case 'verified_warn': return '余额可用（profile 未解析）'
      case 'duplicate': return '重复凭据'
      case 'skipped': return '未处理'
      case 'failed':
        return result.error ? '失败' : '失败（未创建）'
    }
  }

  // 输入侧只检查 JSON 语法；记录数与认证类型由服务端预检给出
  const parseError = useMemo(() => {
    if (!jsonInput.trim()) return ''
    try {
      parseJsonDocument(jsonInput)
      return ''
    } catch (e) {
      return extractErrorMessage(e)
    }
  }, [jsonInput])

  const invalidPreviewCount = preview.filter(p => !p.valid).length

  return (
    <Dialog
      open={open}
      onOpenChange={(newOpen) => {
        if (!newOpen && importing) return
        if (!newOpen) resetForm()
        onOpenChange(newOpen)
      }}
    >
      <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>KAM 账号导入（自动验活）</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">KAM 导出 JSON</label>
            <textarea
              placeholder={'粘贴 Kiro Account Manager 导出的 JSON\n\n支持四种容器格式：平铺单对象、平铺数组、包装对象（version + accounts）、旧版 credentials 嵌套\n\n显式的 authMethod 会被尊重（social / idc / external_idp / api_key）；缺省时由字段形态推断。\nMicrosoft Entra ID / Azure AD 账号请保留 tokenEndpoint 或 issuerUrl，公共客户端无需 clientSecret。\n\n容器判别与逐条预检均在服务端完成，点击导入后显示结果。'}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
              className="flex min-h-[200px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 font-mono"
            />
          </div>

          {/* 解析预览 */}
          {parseError && (
            <div className="text-sm text-red-600 dark:text-red-400">解析失败: {parseError}</div>
          )}
          {preview.length > 0 && (
            <div className="space-y-2">
              <div className="text-sm text-muted-foreground">
                容器格式：{describeContainer(container)}，共 {preview.length} 条
                {invalidPreviewCount > 0 && `（其中 ${invalidPreviewCount} 条预检未通过）`}
              </div>
              <div className="space-y-1 text-xs">
                {preview.map(item => (
                  <div
                    key={item.index}
                    className={
                      item.valid
                        ? 'text-muted-foreground'
                        : 'text-red-600 dark:text-red-400'
                    }
                  >
                    #{item.index + 1} {item.email || item.nickname || item.path} —{' '}
                    {item.valid ? describePreviewItem(item) : item.error}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 导入进度和结果 */}
          {(importing || results.length > 0) && (
            <>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? '导入进度' : '导入完成'}</span>
                  <span>{progress.current} / {progress.total}</span>
                </div>
                <div className="w-full bg-secondary rounded-full h-2">
                  <div
                    className="bg-primary h-2 rounded-full transition-all"
                    style={{ width: `${progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">{currentProcessing}</div>
                )}
              </div>

              <div className="flex gap-4 text-sm">
                <span className="text-green-600 dark:text-green-400">
                  ✓ 成功: {results.filter(r => r.status === 'verified').length}
                </span>
                <span className="text-amber-600 dark:text-amber-400">
                  ⚠ profile: {results.filter(r => r.status === 'verified_warn').length}
                </span>
                <span className="text-yellow-600 dark:text-yellow-400">
                  ⚠ 重复: {results.filter(r => r.status === 'duplicate').length}
                </span>
                <span className="text-red-600 dark:text-red-400">
                  ✗ 失败: {results.filter(r => r.status === 'failed').length}
                </span>
                <span className="text-gray-500">
                  ○ 跳过: {results.filter(r => r.status === 'skipped').length}
                </span>
              </div>

              <div className="border rounded-md divide-y max-h-[300px] overflow-y-auto">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || `账号 #${result.index}`}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">用量: {result.usage}</div>
                        )}
                        {result.profileWarning && (
                          <div className="text-xs text-amber-600 dark:text-amber-400 mt-1">{result.profileWarning}</div>
                        )}
                        {result.hasProfileArn === true && (
                          <div className="text-xs text-muted-foreground mt-1">profileArn: 已就绪{result.provider ? ` · ${result.provider}` : ''}</div>
                        )}
                        {result.error && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">{result.error}</div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => { onOpenChange(false); resetForm() }}
            disabled={importing}
          >
            {importing ? '导入中...' : results.length > 0 ? '关闭' : '取消'}
          </Button>
          {results.length === 0 && (
            <Button
              type="button"
              onClick={handleImport}
              disabled={importing || !jsonInput.trim() || !!parseError}
            >
              开始导入并验活
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
