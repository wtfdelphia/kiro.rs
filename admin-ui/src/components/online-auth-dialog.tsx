import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Loader2, ExternalLink, Copy } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  startBuilderIdLogin,
  pollBuilderIdLogin,
  startIamSsoLogin,
  completeIamSsoLogin,
  importSsoToken,
} from '@/api/credentials'
import { useCredentials } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

interface OnlineAuthDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type Tab = 'builderid' | 'iam' | 'sso'

export function OnlineAuthDialog({ open, onOpenChange }: OnlineAuthDialogProps) {
  const [tab, setTab] = useState<Tab>('builderid')
  const { refetch } = useCredentials()

  const [builderRegion, setBuilderRegion] = useState('')
  const [builderStarting, setBuilderStarting] = useState(false)
  const [builderPolling, setBuilderPolling] = useState(false)
  const [builderSessionId, setBuilderSessionId] = useState<string | null>(null)
  const [userCode, setUserCode] = useState('')
  const [verificationUri, setVerificationUri] = useState('')
  const [pollInterval, setPollInterval] = useState(5)
  const [builderStatus, setBuilderStatus] = useState('')
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pollStop = useRef(false)

  const [startUrl, setStartUrl] = useState('')
  const [iamRegion, setIamRegion] = useState('')
  const [iamStarting, setIamStarting] = useState(false)
  const [iamCompleting, setIamCompleting] = useState(false)
  const [iamSessionId, setIamSessionId] = useState<string | null>(null)
  const [authorizeUrl, setAuthorizeUrl] = useState('')
  const [callbackUrl, setCallbackUrl] = useState('')

  const [bearerTokens, setBearerTokens] = useState('')
  const [ssoRegion, setSsoRegion] = useState('')
  const [ssoImporting, setSsoImporting] = useState(false)

  const clearPoll = () => {
    pollStop.current = true
    if (pollTimer.current) {
      clearTimeout(pollTimer.current)
      pollTimer.current = null
    }
    setBuilderPolling(false)
  }

  const resetAll = () => {
    clearPoll()
    setTab('builderid')
    setBuilderRegion('')
    setBuilderStarting(false)
    setBuilderSessionId(null)
    setUserCode('')
    setVerificationUri('')
    setPollInterval(5)
    setBuilderStatus('')
    setStartUrl('')
    setIamRegion('')
    setIamStarting(false)
    setIamCompleting(false)
    setIamSessionId(null)
    setAuthorizeUrl('')
    setCallbackUrl('')
    setBearerTokens('')
    setSsoRegion('')
    setSsoImporting(false)
  }

  useEffect(() => {
    if (!open) {
      clearPoll()
    }
    return () => clearPoll()
  }, [open])

  const copyText = async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text)
      toast.success(`已复制 ${label}`)
    } catch {
      toast.error('复制失败')
    }
  }

  const schedulePoll = (sessionId: string, intervalSec: number) => {
    if (pollStop.current) return
    pollTimer.current = setTimeout(async () => {
      if (pollStop.current) return
      try {
        const res = await pollBuilderIdLogin(sessionId)
        if (res.completed) {
          setBuilderPolling(false)
          setBuilderStatus('completed')
          toast.success(
            res.email
              ? `BuilderId 登录成功: ${res.email}`
              : `BuilderId 登录成功 (#${res.credentialId})`
          )
          await refetch()
          onOpenChange(false)
          resetAll()
          return
        }
        setBuilderStatus(res.status || 'pending')
        const next = Math.max(1, res.interval || intervalSec)
        setPollInterval(next)
        schedulePoll(sessionId, next)
      } catch (e) {
        setBuilderPolling(false)
        toast.error(`轮询失败: ${extractErrorMessage(e)}`)
      }
    }, Math.max(1, intervalSec) * 1000)
  }

  const handleBuilderStart = async () => {
    setBuilderStarting(true)
    clearPoll()
    pollStop.current = false
    try {
      const res = await startBuilderIdLogin(builderRegion.trim() || undefined)
      setBuilderSessionId(res.sessionId)
      setUserCode(res.userCode)
      setVerificationUri(res.verificationUri)
      setPollInterval(res.interval || 5)
      setBuilderStatus('pending')
      setBuilderPolling(true)
      toast.success('请在浏览器完成授权')
      schedulePoll(res.sessionId, res.interval || 5)
    } catch (e) {
      toast.error(`启动失败: ${extractErrorMessage(e)}`)
    } finally {
      setBuilderStarting(false)
    }
  }

  const handleIamStart = async () => {
    if (!startUrl.trim()) {
      toast.error('请填写 startUrl')
      return
    }
    setIamStarting(true)
    try {
      const res = await startIamSsoLogin(startUrl.trim(), iamRegion.trim() || undefined)
      setIamSessionId(res.sessionId)
      setAuthorizeUrl(res.authorizeUrl)
      toast.success('请打开授权链接完成登录，再粘贴回调 URL')
    } catch (e) {
      toast.error(`启动失败: ${extractErrorMessage(e)}`)
    } finally {
      setIamStarting(false)
    }
  }

  const handleIamComplete = async () => {
    if (!iamSessionId) {
      toast.error('请先启动 IAM SSO')
      return
    }
    if (!callbackUrl.trim()) {
      toast.error('请粘贴回调 URL')
      return
    }
    setIamCompleting(true)
    try {
      const res = await completeIamSsoLogin(iamSessionId, callbackUrl.trim())
      toast.success(res.message || (res.action === 'updated' ? '已更新凭据' : '已添加凭据'))
      await refetch()
      onOpenChange(false)
      resetAll()
    } catch (e) {
      toast.error(`完成失败: ${extractErrorMessage(e)}`)
    } finally {
      setIamCompleting(false)
    }
  }

  const handleSsoImport = async () => {
    if (!bearerTokens.trim()) {
      toast.error('请输入 SSO Bearer Token（可多行）')
      return
    }
    setSsoImporting(true)
    try {
      const res = await importSsoToken(bearerTokens, ssoRegion.trim() || undefined)
      const n = res.accounts?.length || 0
      const errN = res.errors?.length || 0
      if (errN > 0) {
        toast.success(`导入成功 ${n} 个，失败 ${errN} 个`)
      } else {
        toast.success(`成功导入 ${n} 个账号`)
      }
      await refetch()
      onOpenChange(false)
      resetAll()
    } catch (e) {
      toast.error(`导入失败: ${extractErrorMessage(e)}`)
    } finally {
      setSsoImporting(false)
    }
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'builderid', label: 'BuilderId' },
    { id: 'iam', label: 'IAM SSO' },
    { id: 'sso', label: 'SSO Token' },
  ]

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) clearPoll()
        onOpenChange(v)
        if (!v) resetAll()
      }}
    >
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>在线授权添加账号</DialogTitle>
        </DialogHeader>

        <div className="flex gap-2 border-b pb-2">
          {tabs.map((t) => (
            <Button
              key={t.id}
              type="button"
              size="sm"
              variant={tab === t.id ? 'default' : 'outline'}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </Button>
          ))}
        </div>

        <div className="space-y-4 py-3 overflow-y-auto flex-1 pr-1">
          {tab === 'builderid' && (
            <>
              <p className="text-sm text-muted-foreground">
                使用设备码登录 BuilderId，完成后自动走统一 ingest 入库。
              </p>
              <div className="space-y-2">
                <label className="text-sm font-medium">Region（可选）</label>
                <Input
                  value={builderRegion}
                  onChange={(e) => setBuilderRegion(e.target.value)}
                  placeholder="us-east-1"
                  disabled={builderPolling || builderStarting}
                />
              </div>
              {userCode && (
                <div className="rounded-md border p-3 space-y-2 bg-muted/40">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <div className="text-xs text-muted-foreground">用户码</div>
                      <div className="font-mono text-lg tracking-wider">{userCode}</div>
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => copyText(userCode, '用户码')}
                    >
                      <Copy className="h-4 w-4" />
                    </Button>
                  </div>
                  {verificationUri && (
                    <div className="flex items-center gap-2 text-sm">
                      <a
                        href={verificationUri}
                        target="_blank"
                        rel="noreferrer"
                        className="text-primary underline inline-flex items-center gap-1 break-all"
                      >
                        {verificationUri}
                        <ExternalLink className="h-3 w-3 shrink-0" />
                      </a>
                    </div>
                  )}
                  <div className="text-xs text-muted-foreground">
                    状态: {builderStatus || 'pending'} · 轮询间隔 {pollInterval}s
                    {builderSessionId ? ` · session ${builderSessionId.slice(0, 8)}…` : ''}
                  </div>
                </div>
              )}
            </>
          )}

          {tab === 'iam' && (
            <>
              <p className="text-sm text-muted-foreground">
                企业 IAM Identity Center SSO。先启动获取授权链接，浏览器登录后粘贴完整回调 URL。
              </p>
              <div className="space-y-2">
                <label className="text-sm font-medium">startUrl</label>
                <Input
                  value={startUrl}
                  onChange={(e) => setStartUrl(e.target.value)}
                  placeholder="https://my-portal.awsapps.com/start"
                  disabled={!!iamSessionId}
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">Region（可选）</label>
                <Input
                  value={iamRegion}
                  onChange={(e) => setIamRegion(e.target.value)}
                  placeholder="us-east-1"
                  disabled={!!iamSessionId}
                />
              </div>
              {authorizeUrl && (
                <div className="rounded-md border p-3 space-y-2 bg-muted/40">
                  <div className="text-xs text-muted-foreground">授权链接</div>
                  <a
                    href={authorizeUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="text-sm text-primary underline break-all inline-flex items-start gap-1"
                  >
                    {authorizeUrl}
                    <ExternalLink className="h-3 w-3 mt-1 shrink-0" />
                  </a>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => copyText(authorizeUrl, '授权链接')}
                  >
                    <Copy className="h-4 w-4 mr-1" /> 复制链接
                  </Button>
                </div>
              )}
              {iamSessionId && (
                <div className="space-y-2">
                  <label className="text-sm font-medium">回调 URL</label>
                  <Input
                    value={callbackUrl}
                    onChange={(e) => setCallbackUrl(e.target.value)}
                    placeholder="http://127.0.0.1/oauth/callback?code=...&state=..."
                  />
                </div>
              )}
            </>
          )}

          {tab === 'sso' && (
            <>
              <p className="text-sm text-muted-foreground">
                粘贴一个或多个 SSO Bearer Token（每行一个），服务端交换后走 ingest，支持部分成功。
              </p>
              <div className="space-y-2">
                <label className="text-sm font-medium">Bearer Token（可多行）</label>
                <textarea
                  className="flex min-h-[120px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  value={bearerTokens}
                  onChange={(e) => setBearerTokens(e.target.value)}
                  placeholder="eyJhbGciOi...\neyJhbGciOi..."
                />
              </div>
              <div className="space-y-2">
                <label className="text-sm font-medium">Region（可选）</label>
                <Input
                  value={ssoRegion}
                  onChange={(e) => setSsoRegion(e.target.value)}
                  placeholder="us-east-1"
                />
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          {tab === 'builderid' && (
            <Button
              type="button"
              onClick={handleBuilderStart}
              disabled={builderStarting || builderPolling}
            >
              {(builderStarting || builderPolling) && (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              )}
              {builderPolling ? '等待授权…' : '开始登录'}
            </Button>
          )}
          {tab === 'iam' && !iamSessionId && (
            <Button type="button" onClick={handleIamStart} disabled={iamStarting}>
              {iamStarting && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              启动 IAM SSO
            </Button>
          )}
          {tab === 'iam' && iamSessionId && (
            <Button type="button" onClick={handleIamComplete} disabled={iamCompleting}>
              {iamCompleting && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              完成并入库
            </Button>
          )}
          {tab === 'sso' && (
            <Button type="button" onClick={handleSsoImport} disabled={ssoImporting}>
              {ssoImporting && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              导入 Token
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
