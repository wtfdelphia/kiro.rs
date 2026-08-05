import { useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Check, Copy, Loader2, Plug } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { getPublicApi } from '@/api/public-api'
import { extractErrorMessage } from '@/lib/utils'
import type { PublicApiEndpoint, PublicApiResponse, PublicEndpointStatus } from '@/types/api'

interface PublicApiPanelProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

const STATUS_META: Record<PublicEndpointStatus, { label: string; variant: 'success' | 'warning' | 'secondary' }> = {
  live: { label: '可用', variant: 'success' },
  beta: { label: 'Beta', variant: 'warning' },
  planned: { label: '未启用', variant: 'secondary' },
}

/** 客户端接入配方；OpenAI 侧 Base URL 需带 /v1，Anthropic 侧不带 */
const RECIPES: { client: string; baseUrlSuffix: string; path: string; auth: string }[] = [
  { client: 'Anthropic SDK', baseUrlSuffix: '', path: '/v1/messages', auth: 'x-api-key 或 Bearer' },
  { client: 'Claude Code', baseUrlSuffix: '', path: '/cc/v1/messages', auth: 'x-api-key 或 Bearer' },
  { client: 'OpenAI SDK (Chat)', baseUrlSuffix: '/v1', path: '/chat/completions', auth: 'Bearer' },
  { client: 'OpenAI SDK (Responses)', baseUrlSuffix: '/v1', path: '/responses', auth: 'Bearer' },
  { client: 'Models', baseUrlSuffix: '/v1', path: '/models', auth: '同 public API（需鉴权）' },
]

export function PublicApiPanel({ open, onOpenChange }: PublicApiPanelProps) {
  const [loading, setLoading] = useState(false)
  const [data, setData] = useState<PublicApiResponse | null>(null)
  // 仅影响展示与复制文本，不写回服务端
  const [baseUrl, setBaseUrl] = useState('')
  const [copiedKey, setCopiedKey] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    let cancelled = false
    const load = async () => {
      setLoading(true)
      try {
        const resp = await getPublicApi()
        if (cancelled) return
        setData(resp)
        setBaseUrl(resp.server.suggestedBaseUrl ?? window.location.origin)
      } catch (error) {
        if (!cancelled) toast.error(extractErrorMessage(error))
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [open])

  const normalizedBase = useMemo(() => baseUrl.trim().replace(/\/+$/, ''), [baseUrl])

  const copy = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      setCopiedKey(key)
      toast.success('已复制')
      window.setTimeout(() => setCopiedKey((k) => (k === key ? null : k)), 1500)
    } catch {
      toast.error('复制失败，请手动选择文本')
    }
  }

  const fullUrl = (path: string) => `${normalizedBase}${path}`

  /** curl 示例中的 Base URL 替换为当前展示值 */
  const curlWithBase = (endpoint: PublicApiEndpoint) => {
    if (!data) return endpoint.examples.curl
    const serverBase = data.server.suggestedBaseUrl ?? `http://localhost:${data.server.port}`
    return endpoint.examples.curl.split(serverBase.replace(/\/+$/, '')).join(normalizedBase)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Plug className="h-5 w-5" />
            对外 API 端点
          </DialogTitle>
          <DialogDescription>
            客户端接入本代理的端点清单。这里是「客户端 → 本代理」的对外 API，
            与运行时设置里的「Kiro 上游端点」（本代理 → 上游）不是同一回事。
          </DialogDescription>
        </DialogHeader>

        {loading && !data ? (
          <div className="flex items-center justify-center py-10 text-muted-foreground">
            <Loader2 className="h-5 w-5 animate-spin mr-2" />
            加载中...
          </div>
        ) : !data ? (
          <div className="py-10 text-center text-sm text-muted-foreground">暂无数据</div>
        ) : (
          <div className="space-y-6">
            {/* 服务概要 */}
            <section className="space-y-3">
              <h3 className="text-sm font-semibold">服务概要</h3>
              <div className="rounded-lg border p-3 space-y-3">
                <div className="space-y-1.5">
                  <label className="text-xs text-muted-foreground">
                    Base URL（仅影响本页展示与复制内容）
                  </label>
                  <Input
                    value={baseUrl}
                    onChange={(e) => setBaseUrl(e.target.value)}
                    placeholder={window.location.origin}
                  />
                </div>
                <div className="grid gap-2 text-xs sm:grid-cols-2">
                  <div className="text-muted-foreground">
                    监听地址：
                    <span className="text-foreground font-mono">
                      {data.server.listenHost}:{data.server.port}
                    </span>
                  </div>
                  <div className="text-muted-foreground">
                    requireApiKey：
                    <span className="text-foreground">{data.server.requireApiKey ? '开启' : '关闭'}</span>
                  </div>
                  <div className="text-muted-foreground">
                    客户端 API Key：
                    <span className="text-foreground font-mono">
                      {data.server.apiKeyMask ?? '未配置'}
                    </span>
                  </div>
                  <div className="text-muted-foreground">
                    支持鉴权头：
                    <span className="text-foreground font-mono">
                      {data.server.authHeaders.join(' / ')}
                    </span>
                  </div>
                </div>
              </div>
            </section>

            {/* 协议分组 */}
            {data.families.map((family) => (
              <section key={family.family} className="space-y-2">
                <h3 className="text-sm font-semibold">{family.label}</h3>
                <div className="space-y-2">
                  {family.endpoints.map((endpoint) => {
                    const meta = STATUS_META[endpoint.status]
                    return (
                      <div key={endpoint.id} className="rounded-lg border p-3 space-y-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant="outline" className="font-mono">
                            {endpoint.method}
                          </Badge>
                          <code className="text-sm font-mono">{endpoint.path}</code>
                          <Badge variant={meta.variant}>{meta.label}</Badge>
                          {endpoint.stream && <Badge variant="outline">流式</Badge>}
                          <div className="ml-auto flex gap-1">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => copy(`${endpoint.id}:url`, fullUrl(endpoint.path))}
                              title="复制完整 URL"
                            >
                              {copiedKey === `${endpoint.id}:url` ? (
                                <Check className="h-3.5 w-3.5" />
                              ) : (
                                <Copy className="h-3.5 w-3.5" />
                              )}
                              <span className="ml-1 text-xs">URL</span>
                            </Button>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => copy(`${endpoint.id}:curl`, curlWithBase(endpoint))}
                              title="复制 curl 示例"
                            >
                              {copiedKey === `${endpoint.id}:curl` ? (
                                <Check className="h-3.5 w-3.5" />
                              ) : (
                                <Copy className="h-3.5 w-3.5" />
                              )}
                              <span className="ml-1 text-xs">curl</span>
                            </Button>
                          </div>
                        </div>

                        <p className="text-xs text-muted-foreground">{endpoint.summary}</p>

                        {endpoint.status === 'planned' && (
                          <p className="text-xs text-yellow-600 dark:text-yellow-500">
                            尚未启用：该端点已登记但未挂载，现在请求会返回 404。
                          </p>
                        )}

                        {endpoint.clientHints.length > 0 && (
                          <ul className="space-y-0.5 text-xs text-muted-foreground">
                            {endpoint.clientHints.map((hint, i) => (
                              <li key={i}>· {hint}</li>
                            ))}
                          </ul>
                        )}
                      </div>
                    )
                  })}
                </div>
              </section>
            ))}

            {/* 客户端配方 */}
            <section className="space-y-2">
              <h3 className="text-sm font-semibold">客户端配方</h3>
              <div className="rounded-lg border divide-y">
                {RECIPES.map((recipe) => {
                  const base = `${normalizedBase}${recipe.baseUrlSuffix}`
                  return (
                    <div
                      key={recipe.client}
                      className="flex flex-wrap items-center gap-2 p-2.5 text-xs"
                    >
                      <span className="w-44 shrink-0 font-medium">{recipe.client}</span>
                      <code className="font-mono text-muted-foreground">{base}</code>
                      <code className="font-mono">{recipe.path}</code>
                      <span className="text-muted-foreground">{recipe.auth}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="ml-auto"
                        onClick={() => copy(`recipe:${recipe.client}`, base)}
                        title="复制 Base URL"
                      >
                        {copiedKey === `recipe:${recipe.client}` ? (
                          <Check className="h-3.5 w-3.5" />
                        ) : (
                          <Copy className="h-3.5 w-3.5" />
                        )}
                      </Button>
                    </div>
                  )
                })}
              </div>
              <p className="text-xs text-muted-foreground">
                注意 <code className="font-mono">OPENAI_BASE_URL</code> 需带{' '}
                <code className="font-mono">/v1</code> 后缀，
                <code className="font-mono">ANTHROPIC_BASE_URL</code> 不带。这是接入时最高频的错误。
              </p>
            </section>

            {/* 注意区 */}
            <section className="space-y-2">
              <h3 className="text-sm font-semibold">接入须知</h3>
              <ul className="rounded-lg border p-3 space-y-1.5 text-xs text-muted-foreground">
                <li>
                  · <span className="text-foreground">对外 API ≠ Kiro 上游端点</span>
                  ：本页列出的是客户端要连的地址；运行时设置里的默认端点（ide）是本代理连上游用的，
                  两者不可互换。
                </li>
                <li>
                  · <code className="font-mono">/v1/messages</code> 与{' '}
                  <code className="font-mono">/cc/v1/messages</code> 的流式行为不同：前者增量输出，
                  后者缓冲到上游用量事件到达后一次性输出，换取准确的 input_tokens。
                </li>
                <li>
                  · <code className="font-mono">GET /v1/models</code> 需鉴权。未配置 API Key 的客户端
                  若先探测模型列表会得到 401。
                </li>
                <li>
                  · 示例中的 <code className="font-mono">API_KEY</code>{' '}
                  是占位符，请替换为你的客户端 Key（不是 Admin Key）。
                </li>
                <li>· 标记为「未启用」的端点尚未挂载，请求会返回 404。</li>
              </ul>
            </section>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
