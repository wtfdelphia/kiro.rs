import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@testing-library/react'
import { CredentialCard } from './credential-card'
import { makeCredential, renderWithQuery } from '@/test/render'
import type { BalanceResponse, CredentialBalanceSnapshot } from '@/types/api'

vi.mock('@/api/credentials', () => ({
  getCredentialBalance: vi.fn(),
  refreshCredentialModels: vi.fn(),
  getCredentials: vi.fn(),
  setCredentialDisabled: vi.fn(),
  setCredentialPriority: vi.fn(),
  resetCredentialFailure: vi.fn(),
  forceRefreshToken: vi.fn(),
  addCredential: vi.fn(),
  deleteCredential: vi.fn(),
  getLoadBalancingMode: vi.fn(),
  setLoadBalancingMode: vi.fn(),
}))

const getEndpointSettings = vi.hoisted(() => vi.fn())
vi.mock('@/api/settings', () => ({ getEndpointSettings }))

const snapshot: CredentialBalanceSnapshot = {
  subscriptionTitle: 'KIRO FREE',
  currentUsage: 40,
  usageLimit: 100,
  remaining: 60,
  usagePercentage: 40,
  cachedAt: 1_700_000_000,
  ageSecs: 120,
  stale: false,
}

const live: BalanceResponse = {
  id: 1,
  subscriptionTitle: 'KIRO PRO+',
  currentUsage: 10,
  usageLimit: 100,
  remaining: 90,
  usagePercentage: 10,
  nextResetAt: null,
}

beforeEach(() => {
  getEndpointSettings.mockReset()
  getEndpointSettings.mockResolvedValue({ defaultEndpoint: 'ide', registeredEndpoints: ['ide'] })
})

function renderCard(props: Partial<Parameters<typeof CredentialCard>[0]> = {}) {
  return renderWithQuery(
    <CredentialCard
      credential={makeCredential()}
      onViewBalance={vi.fn()}
      selected={false}
      onToggleSelect={vi.fn()}
      balance={null}
      loadingBalance={false}
      {...props}
    />,
  )
}

describe('CredentialCard 余额三态', () => {
  it('cached 态展示数值并标注相对时间', () => {
    renderCard({ credential: makeCredential({ balance: snapshot }) })

    expect(screen.getByText(/60\.00 \/ 100\.00/)).toBeInTheDocument()
    expect(screen.getByText(/缓存于 2 分钟前/)).toBeInTheDocument()
    expect(screen.queryByText('未查询')).not.toBeInTheDocument()
  })

  it('stale 快照仍展示数值并给出过期提示', () => {
    renderCard({
      credential: makeCredential({ balance: { ...snapshot, stale: true, ageSecs: 900 } }),
    })

    expect(screen.getByText(/60\.00 \/ 100\.00/)).toBeInTheDocument()
    expect(screen.getByText(/缓存已过期（15 分钟前）/)).toBeInTheDocument()
  })

  it('none 态显示「未查询」而非「未知」', () => {
    renderCard()

    expect(screen.getByText('未查询')).toBeInTheDocument()
    expect(screen.queryByText(/缓存于/)).not.toBeInTheDocument()
  })

  it('实时结果覆盖缓存后新鲜度标注消失', () => {
    const { unmount } = renderCard({ credential: makeCredential({ balance: snapshot }) })
    expect(screen.getByText(/缓存于 2 分钟前/)).toBeInTheDocument()
    unmount()

    renderCard({ credential: makeCredential({ balance: snapshot }), balance: live })
    expect(screen.getByText(/90\.00 \/ 100\.00/)).toBeInTheDocument()
    expect(screen.queryByText(/缓存于/)).not.toBeInTheDocument()
    expect(screen.queryByText(/缓存已过期/)).not.toBeInTheDocument()
  })
})

describe('CredentialCard 订阅等级取值链', () => {
  it('首屏用顶层 subscriptionTitle，无需额外请求', () => {
    renderCard({ credential: makeCredential({ subscriptionTitle: 'Pro' }) })
    expect(screen.getByText('Pro')).toBeInTheDocument()
  })

  it('顶层值优先于快照内的值', () => {
    renderCard({
      credential: makeCredential({ subscriptionTitle: 'Pro', balance: snapshot }),
    })
    expect(screen.getByText('Pro')).toBeInTheDocument()
    expect(screen.queryByText('KIRO FREE')).not.toBeInTheDocument()
  })

  it('仅快照带订阅等级时回落到快照', () => {
    renderCard({ credential: makeCredential({ balance: snapshot }) })
    expect(screen.getByText('KIRO FREE')).toBeInTheDocument()
    expect(screen.queryByText('未知等级')).not.toBeInTheDocument()
  })

  it('三者全缺显示「未知等级」', () => {
    renderCard()
    expect(screen.getByText('未知等级')).toBeInTheDocument()
  })

  it('实时结果优先级最高', () => {
    renderCard({
      credential: makeCredential({ subscriptionTitle: 'Pro', balance: snapshot }),
      balance: live,
    })
    expect(screen.getByText('KIRO PRO+')).toBeInTheDocument()
    expect(screen.queryByText('Pro')).not.toBeInTheDocument()
  })
})

describe('CredentialCard 标题', () => {
  it('email 与 id 共存', () => {
    renderCard({ credential: makeCredential({ id: 7, email: 'alice@example.com' }) })
    expect(screen.getByText('alice@example.com')).toBeInTheDocument()
    expect(screen.getByText('#7')).toBeInTheDocument()
  })

  it('email 缺失时回落 nickname', () => {
    renderCard({ credential: makeCredential({ id: 8, nickname: 'alice' }) })
    expect(screen.getByText('alice')).toBeInTheDocument()
    expect(screen.getByText('#8')).toBeInTheDocument()
  })

  it('email 与 nickname 缺失时回落 userId', () => {
    renderCard({ credential: makeCredential({ id: 9, userId: 'u-123' }) })
    expect(screen.getByText('u-123')).toBeInTheDocument()
    expect(screen.getByText('#9')).toBeInTheDocument()
  })

  it('身份信息全缺显示「未获取身份」', () => {
    renderCard({ credential: makeCredential({ id: 10 }) })
    expect(screen.getByText('未获取身份')).toBeInTheDocument()
    expect(screen.getByText('#10')).toBeInTheDocument()
  })

  it('长身份名截断并保留完整值于 title，#id 不参与截断', () => {
    const long = `${'a'.repeat(120)}@example.com`
    renderCard({ credential: makeCredential({ id: 11, email: long }) })

    const nameEl = screen.getByText(long)
    expect(nameEl).toHaveAttribute('title', long)
    expect(nameEl.className).toContain('truncate')

    const idEl = screen.getByText('#11')
    expect(idEl.className).toContain('shrink-0')
    expect(idEl.className).not.toContain('truncate')
  })
})

describe('CredentialCard 徽章', () => {
  it('idc + BuilderID 省略 provider 徽章', () => {
    renderCard({ credential: makeCredential({ authMethod: 'idc', provider: 'BuilderID' }) })
    expect(screen.getByText('IdC')).toBeInTheDocument()
    expect(screen.queryByText('BuilderID')).not.toBeInTheDocument()
  })

  it('social + Google 保留 provider 徽章', () => {
    renderCard({ credential: makeCredential({ authMethod: 'social', provider: 'Google' }) })
    expect(screen.getByText('Social')).toBeInTheDocument()
    expect(screen.getByText('Google')).toBeInTheDocument()
  })

  it('external_idp 的 provider 始终保留', () => {
    renderCard({ credential: makeCredential({ authMethod: 'external_idp', provider: 'Azure' }) })
    expect(screen.getByText('外部 IdP')).toBeInTheDocument()
    expect(screen.getByText('Azure')).toBeInTheDocument()
  })

  it('四个类型取值各有可读文案', () => {
    const cases: Array<[string, string]> = [
      ['social', 'Social'],
      ['idc', 'IdC'],
      ['external_idp', '外部 IdP'],
      ['api_key', 'API Key'],
    ]
    for (const [authMethod, label] of cases) {
      const { unmount } = renderCard({ credential: makeCredential({ authMethod }) })
      expect(screen.getByText(label)).toBeInTheDocument()
      expect(screen.queryByText(authMethod)).not.toBeInTheDocument()
      unmount()
    }
  })

  it('endpoint 等于默认值时省略徽章', async () => {
    renderCard({ credential: makeCredential({ endpoint: 'ide' }) })
    await waitFor(() => expect(getEndpointSettings).toHaveBeenCalled())
    await waitFor(() => expect(screen.queryByText('ide')).not.toBeInTheDocument())
  })

  it('endpoint 不同于默认值时渲染徽章', async () => {
    renderCard({ credential: makeCredential({ endpoint: 'custom' }) })
    await waitFor(() => expect(screen.getByText('custom')).toBeInTheDocument())
  })

  it('默认 endpoint 请求失败时保守渲染徽章', async () => {
    getEndpointSettings.mockRejectedValue(new Error('boom'))
    renderCard({ credential: makeCredential({ endpoint: 'ide' }) })
    await waitFor(() => expect(screen.getByText('ide')).toBeInTheDocument())
  })
})
