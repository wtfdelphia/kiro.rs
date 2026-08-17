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
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import {
  getAuthSettings,
  getClientIdentitySettings,
  getEndpointSettings,
  getProxySettings,
  getWebSearchSettings,
  getWebsocketSettings,
  updateAuthSettings,
  updateClientIdentitySettings,
  updateEndpointSettings,
  updateProxySettings,
  updateWebSearchSettings,
  updateWebsocketSettings,
} from '@/api/settings'
import { extractErrorMessage } from '@/lib/utils'
import { bytesToMb, parseWsNumericFields } from '@/lib/websocket-settings'

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

  const [webSearchEmulation, setWebSearchEmulation] = useState(true)

  const [wsEnabled, setWsEnabled] = useState(true)
  const [wsMode, setWsMode] = useState('http_bridge')
  const [wsMaxConnections, setWsMaxConnections] = useState('')
  const [wsFirstMessageTimeout, setWsFirstMessageTimeout] = useState('')
  const [wsInterTurnIdleTimeout, setWsInterTurnIdleTimeout] = useState('')
  const [wsMaxMessageMb, setWsMaxMessageMb] = useState('')
  const [wsUpstreamReadTimeout, setWsUpstreamReadTimeout] = useState('')
  const [wsActiveConnections, setWsActiveConnections] = useState<number | null>(null)
  const [wsLoadError, setWsLoadError] = useState<string | null>(null)

  const [kiroVersion, setKiroVersion] = useState('')
  const [systemVersion, setSystemVersion] = useState('')
  const [nodeVersion, setNodeVersion] = useState('')

  const load = async () => {
    setLoading(true)
    try {
      const [proxy, endpoint, auth, identity, websearch] = await Promise.all([
        getProxySettings(),
        getEndpointSettings(),
        getAuthSettings(),
        getClientIdentitySettings(),
        getWebSearchSettings(),
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
      setKiroVersion(identity.kiroVersion)
      setSystemVersion(identity.systemVersion)
      setNodeVersion(identity.nodeVersion)
      setWebSearchEmulation(websearch.webSearchEmulation)
    } catch (e) {
      toast.error('加载设置失败: ' + extractErrorMessage(e))
    }

    // WebSocket 设置独立加载：失败只影响本分区，其余分区照常可用
    try {
      const ws = await getWebsocketSettings()
      setWsEnabled(ws.enabled)
      setWsMode(ws.mode)
      setWsMaxConnections(String(ws.maxConnections))
      setWsFirstMessageTimeout(String(ws.clientFirstMessageTimeoutSeconds))
      setWsInterTurnIdleTimeout(String(ws.interTurnIdleTimeoutSeconds))
      setWsMaxMessageMb(bytesToMb(ws.maxMessageBytes))
      setWsUpstreamReadTimeout(String(ws.upstreamReadTimeoutSeconds))
      setWsActiveConnections(ws.activeConnections)
      setWsLoadError(null)
    } catch (e) {
      setWsLoadError(extractErrorMessage(e))
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
    // WebSocket 数值字段保存前校验；加载失败的分区不参与保存
    let wsFields: ReturnType<typeof parseWsNumericFields> | null = null
    if (!wsLoadError) {
      wsFields = parseWsNumericFields({
        maxConnections: wsMaxConnections,
        clientFirstMessageTimeoutSeconds: wsFirstMessageTimeout,
        interTurnIdleTimeoutSeconds: wsInterTurnIdleTimeout,
        maxMessageMb: wsMaxMessageMb,
        upstreamReadTimeoutSeconds: wsUpstreamReadTimeout,
      })
      if (!wsFields.ok) {
        toast.error(wsFields.error)
        return
      }
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
      await updateClientIdentitySettings({
        kiroVersion: kiroVersion.trim(),
        systemVersion: systemVersion.trim(),
        nodeVersion: nodeVersion.trim(),
      })
      await updateWebSearchSettings({ webSearchEmulation })
      if (wsFields) {
        await updateWebsocketSettings({
          enabled: wsEnabled,
          mode: wsMode,
          ...wsFields.value,
        })
      }
      toast.success('设置已保存并热更新')
      if (wsLoadError) {
        toast.error('WebSocket 设置加载失败，本次未保存该分区')
      }
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
              <Select
                value={defaultEndpoint}
                onValueChange={setDefaultEndpoint}
                triggerClassName="h-9"
                options={registeredEndpoints.map((ep) => ({ value: ep, label: ep }))}
              />
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-medium">客户端标识</h3>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                <Input
                  placeholder="kiroVersion"
                  value={kiroVersion}
                  onChange={(e) => setKiroVersion(e.target.value)}
                />
                <Input
                  placeholder="systemVersion"
                  value={systemVersion}
                  onChange={(e) => setSystemVersion(e.target.value)}
                />
                <Input
                  placeholder="nodeVersion"
                  value={nodeVersion}
                  onChange={(e) => setNodeVersion(e.target.value)}
                />
              </div>
              <p className="text-xs text-amber-700 dark:text-amber-400">
                这些值会进入后续上游请求指纹；错误版本可能导致上游拒绝。保存后写盘并热生效。
              </p>
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

            <section className="space-y-2">
              <h3 className="text-sm font-medium">Web 搜索代执行</h3>
              <div className="flex items-center justify-between rounded-md border p-3">
                <div>
                  <div className="text-sm">启用 web_search 代执行</div>
                  <div className="text-xs text-muted-foreground">
                    仅影响 <code className="font-mono">/v1/responses</code>：声明单个
                    web_search 工具时由本代理执行搜索。关闭后该工具走正常工具路径，
                    交给模型自行决定。
                  </div>
                </div>
                <Switch
                  checked={webSearchEmulation}
                  onCheckedChange={setWebSearchEmulation}
                />
              </div>
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-medium">WebSocket ingress</h3>
              {wsLoadError ? (
                <p className="text-xs text-red-600 dark:text-red-400">
                  WebSocket 设置加载失败：{wsLoadError}（其余分区不受影响）
                </p>
              ) : (
                <>
                  <div className="flex items-center justify-between rounded-md border p-3">
                    <div>
                      <div className="text-sm">启用 WebSocket ingress</div>
                      <div className="text-xs text-muted-foreground">
                        关闭后新的 <code className="font-mono">GET /v1/responses</code>{' '}
                        upgrade 请求将被 503 拒绝；存量会话不受中断，自然终态。
                      </div>
                    </div>
                    <Switch checked={wsEnabled} onCheckedChange={setWsEnabled} />
                  </div>
                  <div className="grid grid-cols-2 items-center gap-2">
                    <Select
                      value={wsMode}
                      onValueChange={setWsMode}
                      triggerClassName="h-9"
                      options={[
                        { value: 'http_bridge', label: 'http_bridge' },
                        { value: 'passthrough', label: 'passthrough（预留）' },
                      ]}
                    />
                    <div className="text-xs text-muted-foreground">
                      活跃连接：{wsActiveConnections ?? '-'}（只读）
                    </div>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    passthrough 为预留模式，当前选择后每个 turn 将以 501 响应。
                  </p>
                  <div className="grid grid-cols-2 gap-2">
                    <label className="space-y-1 text-xs text-muted-foreground">
                      <span>最大并发连接</span>
                      <Input
                        type="number"
                        min={0}
                        value={wsMaxConnections}
                        onChange={(e) => setWsMaxConnections(e.target.value)}
                      />
                    </label>
                    <label className="space-y-1 text-xs text-muted-foreground">
                      <span>首帧超时（秒）</span>
                      <Input
                        type="number"
                        min={0}
                        value={wsFirstMessageTimeout}
                        onChange={(e) => setWsFirstMessageTimeout(e.target.value)}
                      />
                    </label>
                    <label className="space-y-1 text-xs text-muted-foreground">
                      <span>turn 间空闲超时（秒，0=不启用）</span>
                      <Input
                        type="number"
                        min={0}
                        value={wsInterTurnIdleTimeout}
                        onChange={(e) => setWsInterTurnIdleTimeout(e.target.value)}
                      />
                    </label>
                    <label className="space-y-1 text-xs text-muted-foreground">
                      <span>上游读超时（秒）</span>
                      <Input
                        type="number"
                        min={0}
                        value={wsUpstreamReadTimeout}
                        onChange={(e) => setWsUpstreamReadTimeout(e.target.value)}
                      />
                    </label>
                    <label className="space-y-1 text-xs text-muted-foreground">
                      <span>单帧上限（MB）</span>
                      <Input
                        type="number"
                        min={1}
                        value={wsMaxMessageMb}
                        onChange={(e) => setWsMaxMessageMb(e.target.value)}
                      />
                    </label>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    单帧上限以 MB 编辑、按字节落库；经 API 直设的非整 MB 值会在下次保存时取整。
                  </p>
                </>
              )}
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
