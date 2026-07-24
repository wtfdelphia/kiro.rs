import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import { Loader2, Settings, Save } from 'lucide-react'
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
import { Switch } from '@/components/ui/switch'
import {
  getAuthSettings,
  getEndpointSettings,
  getProxySettings,
  updateAuthSettings,
  updateEndpointSettings,
  updateProxySettings,
} from '@/api/settings'
import { extractErrorMessage } from '@/lib/utils'

interface SettingsPanelProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function SettingsPanel({ open, onOpenChange }: SettingsPanelProps) {
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)

  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [hasProxyAuth, setHasProxyAuth] = useState(false)

  const [defaultEndpoint, setDefaultEndpoint] = useState('ide')
  const [registeredEndpoints, setRegisteredEndpoints] = useState<string[]>(['ide'])

  const [requireApiKey, setRequireApiKey] = useState(true)
  const [apiKeyMask, setApiKeyMask] = useState<string | null>(null)
  const [hasApiKey, setHasApiKey] = useState(false)
  const [newApiKey, setNewApiKey] = useState('')
  const [confirmDisableAuth, setConfirmDisableAuth] = useState(false)

  const load = async () => {
    setLoading(true)
    try {
      const [proxy, endpoint, auth] = await Promise.all([
        getProxySettings(),
        getEndpointSettings(),
        getAuthSettings(),
      ])
      setProxyUrl(proxy.proxyUrl ?? '')
      setProxyUsername(proxy.proxyUsername ?? '')
      setProxyPassword('')
      setHasProxyAuth(proxy.hasProxyAuth)
      setDefaultEndpoint(endpoint.defaultEndpoint)
      setRegisteredEndpoints(endpoint.registeredEndpoints?.length ? endpoint.registeredEndpoints : ['ide'])
      setRequireApiKey(auth.requireApiKey)
      setHasApiKey(auth.hasApiKey)
      setApiKeyMask(auth.apiKeyMask)
      setNewApiKey('')
      setConfirmDisableAuth(false)
    } catch (e) {
      toast.error('加载设置失败: ' + extractErrorMessage(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (open) void load()
  }, [open])

  const handleSave = async () => {
    if (!requireApiKey && !confirmDisableAuth) {
      toast.error('关闭客户端 API Key 校验前请勾选二次确认')
      return
    }
    setSaving(true)
    try {
      await updateProxySettings({
        proxyUrl: proxyUrl.trim() || null,
        proxyUsername: proxyUsername.trim() || null,
        proxyPassword: proxyPassword ? proxyPassword : null,
      })
      await updateEndpointSettings({ defaultEndpoint })
      await updateAuthSettings({
        requireApiKey,
        apiKey: newApiKey.trim() ? newApiKey.trim() : undefined,
      })
      toast.success('设置已保存并热更新')
      await load()
    } catch (e) {
      toast.error('保存设置失败: ' + extractErrorMessage(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings className="h-5 w-5" />
            运行时设置
          </DialogTitle>
          <DialogDescription>
            出站代理、默认 Kiro 端点与客户端 API Key 校验。保存后写盘并热更新，无需重启。
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex items-center justify-center py-10 text-muted-foreground">
            <Loader2 className="h-5 w-5 animate-spin mr-2" />
            加载中…
          </div>
        ) : (
          <div className="space-y-5">
            <section className="space-y-2">
              <h3 className="text-sm font-medium">出站代理</h3>
              <Input
                placeholder="http://127.0.0.1:7890 或 socks5://…（空=清除）"
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="代理用户名（可选）"
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                />
                <Input
                  type="password"
                  placeholder={hasProxyAuth ? '密码已配置，留空保持' : '代理密码（可选）'}
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                凭据级 proxy 仍优先于全局代理。
              </p>
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-medium">默认 Kiro 端点</h3>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm"
                value={defaultEndpoint}
                onChange={(e) => setDefaultEndpoint(e.target.value)}
              >
                {registeredEndpoints.map((ep) => (
                  <option key={ep} value={ep}>
                    {ep}
                  </option>
                ))}
              </select>
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-medium">客户端 API Key</h3>
              <div className="flex items-center justify-between rounded-md border p-3">
                <div>
                  <div className="text-sm">要求 API Key</div>
                  <div className="text-xs text-muted-foreground">
                    关闭后客户端可不带 key（Admin 仍需 adminApiKey）
                  </div>
                </div>
                <Switch
                  checked={requireApiKey}
                  onCheckedChange={(v) => {
                    setRequireApiKey(v)
                    if (v) setConfirmDisableAuth(false)
                  }}
                />
              </div>
              {!requireApiKey && (
                <label className="flex items-start gap-2 text-sm text-amber-700 dark:text-amber-400">
                  <input
                    type="checkbox"
                    className="mt-1"
                    checked={confirmDisableAuth}
                    onChange={(e) => setConfirmDisableAuth(e.target.checked)}
                  />
                  我确认关闭客户端鉴权可能使服务裸奔
                </label>
              )}
              <div className="text-xs text-muted-foreground">
                当前：{hasApiKey ? apiKeyMask || '已配置' : '未配置'}
              </div>
              <Input
                type="password"
                placeholder="轮换 apiKey（留空不改）"
                value={newApiKey}
                onChange={(e) => setNewApiKey(e.target.value)}
              />
            </section>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            关闭
          </Button>
          <Button onClick={handleSave} disabled={loading || saving}>
            {saving ? (
              <>
                <Loader2 className="h-4 w-4 mr-1 animate-spin" />
                保存中…
              </>
            ) : (
              <>
                <Save className="h-4 w-4 mr-1" />
                保存
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
