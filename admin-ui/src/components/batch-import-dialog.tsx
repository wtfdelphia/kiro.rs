
import { useState } from 'react'
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
import { importCredentialsBatch, type BatchImportItemResult } from '@/api/credentials'
import type { AddCredentialRequest } from '@/types/api'
import { extractErrorMessage, sha256Hex } from '@/lib/utils'

interface BatchImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface CredentialInput {
  refreshToken?: string
  clientId?: string
  clientSecret?: string
  region?: string
  authRegion?: string
  apiRegion?: string
  priority?: number
  machineId?: string
  kiroApiKey?: string
  authMethod?: string
  endpoint?: string
  userId?: string
  nickname?: string
  startUrl?: string
  provider?: string
  profileArn?: string
  email?: string
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verified' | 'verified_warn' | 'duplicate' | 'failed' | 'updated'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  warning?: string
}

const CHUNK_SIZE = 20

export function BatchImportDialog({ open, onOpenChange }: BatchImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState('')
  const [results, setResults] = useState<VerificationResult[]>([])

  const { data: existingCredentials, refetch } = useCredentials()

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  const toRequest = (cred: CredentialInput): AddCredentialRequest => {
    const isApiKey = !!(cred.kiroApiKey?.trim()) || cred.authMethod === 'api_key'
    if (isApiKey) {
      return {
        authMethod: 'api_key',
        kiroApiKey: cred.kiroApiKey?.trim(),
        priority: cred.priority || 0,
        authRegion: cred.authRegion?.trim() || cred.region?.trim() || undefined,
        apiRegion: cred.apiRegion?.trim() || undefined,
        machineId: cred.machineId?.trim() || undefined,
        endpoint: cred.endpoint?.trim() || undefined,
        userId: cred.userId?.trim() || undefined,
        nickname: cred.nickname?.trim() || undefined,
        email: cred.email?.trim() || undefined,
      }
    }
    return {
      refreshToken: cred.refreshToken?.trim(),
      authMethod:
        (cred.authMethod as AddCredentialRequest['authMethod']) ||
        (cred.clientId && cred.clientSecret ? 'idc' : 'social'),
      clientId: cred.clientId?.trim() || undefined,
      clientSecret: cred.clientSecret?.trim() || undefined,
      authRegion: cred.authRegion?.trim() || cred.region?.trim() || undefined,
      apiRegion: cred.apiRegion?.trim() || undefined,
      priority: cred.priority || 0,
      machineId: cred.machineId?.trim() || undefined,
      endpoint: cred.endpoint?.trim() || undefined,
      provider: cred.provider?.trim() || undefined,
      profileArn: cred.profileArn?.trim() || undefined,
      userId: cred.userId?.trim() || undefined,
      nickname: cred.nickname?.trim() || undefined,
      startUrl: cred.startUrl?.trim() || undefined,
      email: cred.email?.trim() || undefined,
    }
  }

  const mapServerResult = (
    r: BatchImportItemResult,
    displayIndex: number
  ): VerificationResult => {
    let status: VerificationResult['status'] = 'failed'
    if (r.status === 'duplicate') status = 'duplicate'
    else if (r.status === 'updated') status = r.warning ? 'verified_warn' : 'updated'
    else if (r.status === 'created') status = r.warning ? 'verified_warn' : 'verified'
    else if (r.credentialId) status = r.warning ? 'verified_warn' : 'verified'

    return {
      index: displayIndex,
      status,
      error: r.error,
      email: r.email,
      credentialId: r.credentialId,
      warning: r.warning,
      usage: r.balance ? r.balance.currentUsage + '/' + r.balance.usageLimit : undefined,
    }
  }

  const handleBatchImport = async () => {
    let credentials: CredentialInput[]
    try {
      const parsed = JSON.parse(jsonInput)
      credentials = Array.isArray(parsed) ? parsed : [parsed]
    } catch (error) {
      toast.error('JSON 格式错误: ' + extractErrorMessage(error))
      return
    }

    if (credentials.length === 0) {
      toast.error('没有可导入的凭据')
      return
    }

    try {
      setImporting(true)
      setProgress({ current: 0, total: credentials.length })

      const preResults: VerificationResult[] = credentials.map((_, i) => ({
        index: i + 1,
        status: 'pending',
      }))
      setResults([...preResults])

      const existingOauthHashes = new Set(
        existingCredentials?.credentials
          .map((c) => c.refreshTokenHash)
          .filter((h): h is string => Boolean(h)) || []
      )
      const existingApiKeyHashes = new Set(
        existingCredentials?.credentials
          .map((c) => c.apiKeyHash)
          .filter((h): h is string => Boolean(h)) || []
      )

      const items: AddCredentialRequest[] = []
      const indexMap: number[] = []

      for (let i = 0; i < credentials.length; i++) {
        const cred = credentials[i]
        const isApiKey = !!(cred.kiroApiKey?.trim()) || cred.authMethod === 'api_key'
        preResults[i] = { ...preResults[i], status: 'checking' }
        setResults([...preResults])
        setCurrentProcessing('预检 ' + (i + 1) + '/' + credentials.length)

        if (isApiKey) {
          const apiKey = cred.kiroApiKey?.trim() || ''
          if (!apiKey) {
            preResults[i] = { ...preResults[i], status: 'failed', error: '缺少 kiroApiKey' }
            continue
          }
          const hash = await sha256Hex(apiKey)
          if (existingApiKeyHashes.has(hash)) {
            preResults[i] = { ...preResults[i], status: 'duplicate', error: '该凭据已存在' }
            continue
          }
        } else {
          const token = cred.refreshToken?.trim() || ''
          if (!token) {
            preResults[i] = { ...preResults[i], status: 'failed', error: '缺少 refreshToken' }
            continue
          }
          const hash = await sha256Hex(token)
          if (existingOauthHashes.has(hash)) {
            preResults[i] = { ...preResults[i], status: 'duplicate', error: '该凭据已存在' }
            continue
          }
        }

        items.push(toRequest(cred))
        indexMap.push(i)
      }
      setResults([...preResults])

      let okCount = 0
      let warnCount = 0
      let duplicateCount = preResults.filter((r) => r.status === 'duplicate').length
      let failCount = preResults.filter((r) => r.status === 'failed').length

      for (let offset = 0; offset < items.length; offset += CHUNK_SIZE) {
        const chunk = items.slice(offset, offset + CHUNK_SIZE)
        const chunkIndexes = indexMap.slice(offset, offset + CHUNK_SIZE)
        setCurrentProcessing(
          '服务端导入 ' +
            (offset + 1) +
            '-' +
            Math.min(offset + chunk.length, items.length) +
            ' / ' +
            items.length
        )

        const resp = await importCredentialsBatch({
          items: chunk,
          options: {
            onConflict: 'upsert',
            stopOnError: false,
            fetchBalance: true,
            concurrency: 1,
          },
        })

        for (const r of resp.results) {
          const originalIndex = chunkIndexes[r.index]
          const ui = mapServerResult(r, originalIndex + 1)
          preResults[originalIndex] = ui
          if (ui.status === 'verified' || ui.status === 'updated') okCount++
          else if (ui.status === 'verified_warn') {
            okCount++
            warnCount++
          } else if (ui.status === 'duplicate') duplicateCount++
          else if (ui.status === 'failed') failCount++
        }

        setResults([...preResults])
        setProgress({
          current: Math.min(
            credentials.length - items.length + offset + chunk.length,
            credentials.length
          ),
          total: credentials.length,
        })
      }

      setProgress({ current: credentials.length, total: credentials.length })
      setCurrentProcessing('')
      await refetch()

      if (okCount > 0) {
        toast.success(
          '导入完成：成功 ' +
            okCount +
            (warnCount ? '（含 ' + warnCount + ' 条 profile 警告）' : '') +
            '，重复 ' +
            duplicateCount +
            '，失败 ' +
            failCount
        )
      } else {
        toast.error('导入失败：重复 ' + duplicateCount + '，失败 ' + failCount)
      }
    } catch (error) {
      toast.error('批量导入失败: ' + extractErrorMessage(error))
    } finally {
      setImporting(false)
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'verified':
      case 'updated':
        return <CheckCircle2 className="h-4 w-4 text-green-600" />
      case 'verified_warn':
      case 'duplicate':
        return <AlertCircle className="h-4 w-4 text-yellow-600" />
      case 'failed':
        return <XCircle className="h-4 w-4 text-red-600" />
      default:
        return <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'verified':
        return '成功'
      case 'updated':
        return '已更新'
      case 'verified_warn':
        return '成功（缺 profile）'
      case 'duplicate':
        return '重复'
      case 'failed':
        return '失败'
      case 'checking':
        return '检查中'
      default:
        return '等待'
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>批量导入凭据</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 overflow-y-auto flex-1">
          <div className="space-y-2">
            <textarea
              className="w-full min-h-[160px] rounded-md border bg-background p-3 text-sm font-mono"
              placeholder="JSON 数组或对象，支持 refreshToken / kiroApiKey / userId 等字段"
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
            />
            <p className="text-xs text-muted-foreground">
              主路径走服务端 batch import（默认 upsert）；客户端仅做空字段与 hash 预检。
            </p>
          </div>

          {(importing || results.length > 0) && (
            <>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? '导入进度' : '导入完成'}</span>
                  <span>
                    {progress.current} / {progress.total}
                  </span>
                </div>
                <div className="w-full bg-secondary rounded-full h-2">
                  <div
                    className="bg-primary h-2 rounded-full transition-all"
                    style={{
                      width: (progress.total ? (progress.current / progress.total) * 100 : 0) + '%',
                    }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">{currentProcessing}</div>
                )}
              </div>

              <div className="flex flex-wrap gap-4 text-sm">
                <span className="text-green-600 dark:text-green-400">
                  成功:{' '}
                  {results.filter((r) => r.status === 'verified' || r.status === 'updated').length}
                </span>
                <span className="text-yellow-600 dark:text-yellow-400">
                  警告/重复:{' '}
                  {results.filter((r) => r.status === 'verified_warn' || r.status === 'duplicate').length}
                </span>
                <span className="text-red-600 dark:text-red-400">
                  失败: {results.filter((r) => r.status === 'failed').length}
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
                            {result.email || '凭据 #' + result.index}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">
                            用量: {result.usage}
                          </div>
                        )}
                        {result.warning && (
                          <div className="text-xs text-yellow-600 dark:text-yellow-400 mt-1">
                            {result.warning}
                          </div>
                        )}
                        {result.error && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">
                            {result.error}
                          </div>
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
            onClick={() => {
              onOpenChange(false)
              resetForm()
            }}
            disabled={importing}
          >
            {importing ? '导入中...' : results.length > 0 ? '关闭' : '取消'}
          </Button>
          {results.length === 0 && (
            <Button
              type="button"
              onClick={handleBatchImport}
              disabled={importing || !jsonInput.trim()}
            >
              开始批量导入
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
