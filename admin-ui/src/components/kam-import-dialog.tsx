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
import { importCredentialsBatch } from '@/api/credentials'
import { extractErrorMessage, sha256Hex } from '@/lib/utils'

interface KamImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

// KAM 导出 JSON 中的账号结构
interface KamAccount {
  email?: string
  userId?: string | null
  nickname?: string
  credentials: {
    refreshToken: string
    clientId?: string
    clientSecret?: string
    region?: string
    authMethod?: string
    provider?: string
    profileArn?: string
    startUrl?: string
  }
  machineId?: string
  status?: string
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
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}



// 兼容 KAM 1.8.3 新版平铺格式，统一转换为旧格式（credentials 嵌套结构）
function normalizeKamAccount(item: unknown): unknown {
  if (typeof item !== 'object' || item === null) return item
  const obj = item as Record<string, unknown>
  // 新格式：refreshToken 直接在账号对象上，无 credentials 嵌套
  if (typeof obj.refreshToken === 'string' && typeof obj.credentials === 'undefined') {
    const email = typeof obj.email === 'string' ? obj.email : undefined
    const userId =
      typeof obj.userId === 'string' || obj.userId === null ? (obj.userId as string | null) : undefined
    const nickname =
      typeof obj.nickname === 'string'
        ? obj.nickname
        : typeof obj.label === 'string'
          ? (obj.label as string)
          : undefined
    const status = typeof obj.status === 'string' ? obj.status : undefined
    const machineId = typeof obj.machineId === 'string' ? obj.machineId : undefined
    const clientId = typeof obj.clientId === 'string' ? obj.clientId : undefined
    const clientSecret = typeof obj.clientSecret === 'string' ? obj.clientSecret : undefined
    const region = typeof obj.region === 'string' ? obj.region : undefined
    const authMethod = typeof obj.authMethod === 'string' ? obj.authMethod : undefined
    const provider = typeof obj.provider === 'string' ? obj.provider : undefined
    const profileArn = typeof obj.profileArn === 'string' ? obj.profileArn : undefined
    const startUrl = typeof obj.startUrl === 'string' ? obj.startUrl : undefined

    return {
      email,
      userId,
      nickname,
      status,
      machineId,
      credentials: {
        refreshToken: obj.refreshToken,
        clientId,
        clientSecret,
        region,
        authMethod,
        provider,
        profileArn,
        startUrl,
      },
    }
  }
  return item
}

// 校验元素是否为有效的 KAM 账号结构
function isValidKamAccount(item: unknown): item is KamAccount {
  if (typeof item !== 'object' || item === null) return false
  const obj = item as Record<string, unknown>
  if (typeof obj.credentials !== 'object' || obj.credentials === null) return false
  const cred = obj.credentials as Record<string, unknown>
  return typeof cred.refreshToken === 'string' && cred.refreshToken.trim().length > 0
}

// 解析 KAM 导出 JSON，支持单账号和多账号格式
function parseKamJson(raw: string): KamAccount[] {
  const parsed = JSON.parse(raw)

  let rawItems: unknown[]

  // 标准 KAM 导出格式：{ version, accounts: [...] }
  if (parsed.accounts && Array.isArray(parsed.accounts)) {
    rawItems = parsed.accounts
  }
  // 直接数组（含 KAM 1.8.3 新版平铺格式）
  else if (Array.isArray(parsed)) {
    rawItems = parsed
  }
  // 单个账号对象（旧格式，有 credentials 字段）
  else if (parsed.credentials && typeof parsed.credentials === 'object') {
    rawItems = [parsed]
  }
  // 单个账号对象（新格式，refreshToken 平铺）
  else if (typeof parsed.refreshToken === 'string') {
    rawItems = [parsed]
  }
  else {
    throw new Error('无法识别的 KAM JSON 格式')
  }

  // 兼容新格式：将平铺账号统一转换为 credentials 嵌套结构
  const normalizedItems = rawItems.map(normalizeKamAccount)
  const validAccounts = normalizedItems.filter(isValidKamAccount)

  if (rawItems.length > 0 && validAccounts.length === 0) {
    throw new Error(`共 ${rawItems.length} 条记录，但均缺少有效的 credentials.refreshToken`)
  }

  if (validAccounts.length < rawItems.length) {
    const skipped = rawItems.length - validAccounts.length
    console.warn(`KAM 导入：跳过 ${skipped} 条缺少有效 credentials.refreshToken 的记录`)
  }

  return validAccounts
}

export function KamImportDialog({ open, onOpenChange }: KamImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [skipErrorAccounts, setSkipErrorAccounts] = useState(true)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])


  const { data: existingCredentials, refetch } = useCredentials()

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  const handleImport = async () => {
    let validAccounts: KamAccount[]
    try {
      const accounts = parseKamJson(jsonInput)
      if (accounts.length === 0) {
        toast.error('没有可导入的账号')
        return
      }
      validAccounts = accounts.filter(a => a.credentials?.refreshToken)
      if (validAccounts.length === 0) {
        toast.error('没有包含有效 refreshToken 的账号')
        return
      }
    } catch (error) {
      toast.error('JSON 格式错误: ' + extractErrorMessage(error))
      return
    }

    try {
      setImporting(true)
      setProgress({ current: 0, total: validAccounts.length })

      const initialResults: VerificationResult[] = validAccounts.map((account, i) => {
        if (skipErrorAccounts && account.status === 'error') {
          return { index: i + 1, status: 'skipped' as const, email: account.email || account.nickname }
        }
        return { index: i + 1, status: 'pending' as const, email: account.email || account.nickname }
      })
      setResults(initialResults)

      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      const payloadItems: import('@/types/api').AddCredentialRequest[] = []
      const indexMap: number[] = []
      const resultsArr = [...initialResults]

      let skippedCount = 0
      let duplicateCount = 0
      let failCount = 0

      for (let i = 0; i < validAccounts.length; i++) {
        const account = validAccounts[i]
        if (skipErrorAccounts && account.status === 'error') {
          skippedCount++
          continue
        }
        const cred = account.credentials
        const token = cred.refreshToken.trim()
        const tokenHash = await sha256Hex(token)
        resultsArr[i] = { ...resultsArr[i], status: 'checking' }
        setResults([...resultsArr])
        setCurrentProcessing('预检 ' + (account.email || account.nickname || ('账号 ' + (i + 1))))

        if (existingTokenHashes.has(tokenHash)) {
          duplicateCount++
          const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === tokenHash)
          resultsArr[i] = {
            ...resultsArr[i],
            status: 'duplicate',
            error: '该凭据已存在',
            email: existingCred?.email || account.email,
          }
          continue
        }

        const clientId = cred.clientId?.trim() || undefined
        const clientSecret = cred.clientSecret?.trim() || undefined
        const authMethod = (clientId && clientSecret ? 'idc' : 'social') as 'idc' | 'social'
        if (authMethod === 'social' && (clientId || clientSecret)) {
          failCount++
          resultsArr[i] = {
            ...resultsArr[i],
            status: 'failed',
            error: 'idc 模式需要同时提供 clientId 和 clientSecret',
          }
          continue
        }

        const provider =
          cred.provider?.trim() ||
          (authMethod === 'idc' ? 'BuilderId' : undefined)

        payloadItems.push({
          refreshToken: token,
          authMethod,
          provider,
          profileArn: cred.profileArn?.trim() || undefined,
          authRegion: cred.region?.trim() || undefined,
          clientId,
          clientSecret,
          machineId: account.machineId?.trim() || undefined,
          email: account.email,
          userId: typeof account.userId === 'string' ? account.userId : undefined,
          nickname: account.nickname,
          startUrl: cred.startUrl?.trim() || undefined,
        })
        indexMap.push(i)
        existingTokenHashes.add(tokenHash)
      }
      setResults([...resultsArr])

      let successCount = 0
      const CHUNK = 20
      for (let offset = 0; offset < payloadItems.length; offset += CHUNK) {
        const chunk = payloadItems.slice(offset, offset + CHUNK)
        const chunkIdx = indexMap.slice(offset, offset + CHUNK)
        setCurrentProcessing(
          '服务端导入 ' + (offset + 1) + '-' + Math.min(offset + chunk.length, payloadItems.length)
        )
        for (const oi of chunkIdx) {
          resultsArr[oi] = { ...resultsArr[oi], status: 'verifying' }
        }
        setResults([...resultsArr])

        const resp = await importCredentialsBatch({
          items: chunk,
          options: { onConflict: 'upsert', stopOnError: false, fetchBalance: true, concurrency: 1 },
        })

        for (const r of resp.results) {
          const oi = chunkIdx[r.index]
          const hasWarn = !!r.warning
          let status: VerificationResult['status'] = 'failed'
          if (r.status === 'duplicate') status = 'duplicate'
          else if (r.status === 'updated' || r.status === 'created') status = hasWarn ? 'verified_warn' : 'verified'
          else if (r.credentialId) status = hasWarn ? 'verified_warn' : 'verified'

          if (status === 'verified' || status === 'verified_warn') successCount++
          else if (status === 'duplicate') duplicateCount++
          else failCount++

          resultsArr[oi] = {
            ...resultsArr[oi],
            status,
            error: r.error,
            email: r.email || resultsArr[oi].email,
            credentialId: r.credentialId,
            usage: r.balance ? r.balance.currentUsage + '/' + r.balance.usageLimit : undefined,
            profileWarning: r.warning || undefined,
            hasProfileArn: r.warning ? false : r.credentialId ? true : undefined,
          }
        }
        setResults([...resultsArr])
        setProgress({
          current: Math.min(validAccounts.length, offset + chunk.length + skippedCount),
          total: validAccounts.length,
        })
      }

      setProgress({ current: validAccounts.length, total: validAccounts.length })
      setCurrentProcessing('')
      await refetch()

      if (successCount > 0) {
        toast.success(
          'KAM 导入完成：成功 ' +
            successCount +
            '，重复 ' +
            duplicateCount +
            '，失败 ' +
            failCount +
            (skippedCount ? '，跳过 ' + skippedCount : '')
        )
      } else {
        toast.error('KAM 导入失败：重复 ' + duplicateCount + '，失败 ' + failCount)
      }
    } catch (error) {
      toast.error('导入失败: ' + extractErrorMessage(error))
    } finally {
      setImporting(false)
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
      case 'skipped': return '已跳过（error 状态）'
      case 'failed':
        if (result.rollbackStatus === 'success') return '验活失败（已排除）'
        if (result.rollbackStatus === 'failed') return '验活失败（未排除）'
        return '验活失败（未创建）'
    }
  }

  // 预览解析结果
  const { previewAccounts, parseError } = useMemo(() => {
    if (!jsonInput.trim()) return { previewAccounts: [] as KamAccount[], parseError: '' }
    try {
      return { previewAccounts: parseKamJson(jsonInput), parseError: '' }
    } catch (e) {
      return { previewAccounts: [] as KamAccount[], parseError: extractErrorMessage(e) }
    }
  }, [jsonInput])

  const errorAccountCount = previewAccounts.filter(a => a.status === 'error').length

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
              placeholder={'粘贴 Kiro Account Manager 导出的 JSON\n\n支持 KAM 1.8.3+ 新版平铺格式：\n[\n  {\n    "email": "...",\n    "refreshToken": "...",\n    "clientId": "...",\n    "clientSecret": "...",\n    "region": "us-east-1"\n  }\n]\n\n（可选的 authMethod 字段会被忽略，系统会根据 clientId/clientSecret 自动判断）\n\n也支持旧版嵌套格式：\n{\n  "version": "1.5.0",\n  "accounts": [\n    {\n      "email": "...",\n      "credentials": {\n        "refreshToken": "...",\n        "clientId": "...",\n        "clientSecret": "...",\n        "region": "us-east-1"\n      }\n    }\n  ]\n}'}
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
          {previewAccounts.length > 0 && !importing && results.length === 0 && (
            <div className="space-y-2">
              <div className="text-sm text-muted-foreground">
                识别到 {previewAccounts.length} 个账号
                {errorAccountCount > 0 && `（其中 ${errorAccountCount} 个为 error 状态）`}
              </div>
              {errorAccountCount > 0 && (
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={skipErrorAccounts}
                    onChange={(e) => setSkipErrorAccounts(e.target.checked)}
                    className="rounded border-gray-300"
                  />
                  跳过 error 状态的账号
                </label>
              )}
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
                        {result.rollbackError && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">回滚失败: {result.rollbackError}</div>
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
              disabled={importing || !jsonInput.trim() || previewAccounts.length === 0 || !!parseError}
            >
              开始导入并验活
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
