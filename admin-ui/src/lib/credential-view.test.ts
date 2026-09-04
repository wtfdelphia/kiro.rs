import { describe, expect, it } from 'vitest'
import {
  formatAuthMethod,
  formatCacheAge,
  isProviderRedundant,
  resolveBalanceView,
  resolveIdentityName,
  resolveSubscriptionTitle,
  shouldShowEndpointBadge,
} from './credential-view'
import type { BalanceResponse, CredentialBalanceSnapshot } from '@/types/api'

const live: BalanceResponse = {
  id: 1,
  subscriptionTitle: 'KIRO PRO+',
  currentUsage: 10,
  usageLimit: 100,
  remaining: 90,
  usagePercentage: 10,
  nextResetAt: null,
}

const snapshot: CredentialBalanceSnapshot = {
  subscriptionTitle: 'KIRO FREE',
  currentUsage: 40,
  usageLimit: 100,
  remaining: 60,
  usagePercentage: 40,
  nextResetAt: 1_800_000_000,
  cachedAt: 1_700_000_000,
  ageSecs: 120,
  stale: false,
}

describe('resolveBalanceView', () => {
  it('有实时结果时为 live 态，不带新鲜度信息', () => {
    const view = resolveBalanceView(live, snapshot)
    expect(view.source).toBe('live')
    expect(view).not.toHaveProperty('ageSecs')
    expect(view).not.toHaveProperty('stale')
    if (view.source === 'live') expect(view.remaining).toBe(90)
  })

  it('只有内联快照时为 cached 态，带新鲜度信息', () => {
    const view = resolveBalanceView(null, snapshot)
    expect(view.source).toBe('cached')
    if (view.source === 'cached') {
      expect(view.remaining).toBe(60)
      expect(view.ageSecs).toBe(120)
      expect(view.stale).toBe(false)
    }
  })

  it('过期快照仍带出数值，stale 为 true', () => {
    const view = resolveBalanceView(null, { ...snapshot, stale: true })
    expect(view.source).toBe('cached')
    if (view.source === 'cached') {
      expect(view.stale).toBe(true)
      expect(view.remaining).toBe(60)
    }
  })

  it('两者全缺时为 none 态', () => {
    expect(resolveBalanceView(null, null).source).toBe('none')
    expect(resolveBalanceView(undefined, undefined).source).toBe('none')
  })
})

describe('formatCacheAge', () => {
  it('按秒 / 分 / 小时 / 天分级', () => {
    expect(formatCacheAge(5)).toBe('5 秒前')
    expect(formatCacheAge(120)).toBe('2 分钟前')
    expect(formatCacheAge(7200)).toBe('2 小时前')
    expect(formatCacheAge(172800)).toBe('2 天前')
  })

  it('负数归零', () => {
    expect(formatCacheAge(-3)).toBe('0 秒前')
  })
})

describe('resolveSubscriptionTitle', () => {
  it('实时结果优先级最高', () => {
    expect(resolveSubscriptionTitle(live, 'Pro', 'Free')).toBe('KIRO PRO+')
  })

  it('顶层值优先于快照内的值', () => {
    expect(resolveSubscriptionTitle(null, 'Pro', 'Free')).toBe('Pro')
  })

  it('仅快照有值时回落到快照', () => {
    expect(resolveSubscriptionTitle(null, null, 'Free')).toBe('Free')
    expect(resolveSubscriptionTitle(null, undefined, 'Free')).toBe('Free')
  })

  it('三者全缺返回 null', () => {
    expect(resolveSubscriptionTitle(null, null, null)).toBeNull()
    expect(resolveSubscriptionTitle({ ...live, subscriptionTitle: null }, null, null)).toBeNull()
  })
})

describe('resolveIdentityName', () => {
  it('email 优先', () => {
    expect(
      resolveIdentityName({ email: 'alice@example.com', nickname: 'alice', userId: 'u-1' }),
    ).toBe('alice@example.com')
  })

  it('无 email 回落 nickname', () => {
    expect(resolveIdentityName({ nickname: 'alice', userId: 'u-1' })).toBe('alice')
  })

  it('只有 userId 时回落 userId', () => {
    expect(resolveIdentityName({ userId: 'u-1' })).toBe('u-1')
  })

  it('三者全缺显示「未获取身份」', () => {
    expect(resolveIdentityName({})).toBe('未获取身份')
    expect(resolveIdentityName({ email: '', nickname: null, userId: undefined })).toBe('未获取身份')
  })
})

describe('formatAuthMethod', () => {
  it('四个取值都有可读文案，无一落裸值', () => {
    expect(formatAuthMethod('social')).toBe('Social')
    expect(formatAuthMethod('idc')).toBe('IdC')
    expect(formatAuthMethod('external_idp')).toBe('外部 IdP')
    expect(formatAuthMethod('api_key')).toBe('API Key')
  })

  it('未知取值回落裸值', () => {
    expect(formatAuthMethod('mystery')).toBe('mystery')
  })
})

describe('isProviderRedundant', () => {
  it('idc + BuilderID 判为可推导（大小写与连字符不敏感）', () => {
    expect(isProviderRedundant('idc', 'BuilderID')).toBe(true)
    expect(isProviderRedundant('idc', 'builder-id')).toBe(true)
    expect(isProviderRedundant('idc', 'IAM')).toBe(true)
    expect(isProviderRedundant('idc', 'awsbuilderid')).toBe(true)
  })

  it('idc + 未知 provider 保留', () => {
    expect(isProviderRedundant('idc', 'SomeCorp')).toBe(false)
  })

  it('social + Google 保留', () => {
    expect(isProviderRedundant('social', 'Google')).toBe(false)
  })

  it('external_idp 恒判为不可推导', () => {
    expect(isProviderRedundant('external_idp', 'Azure')).toBe(false)
    expect(isProviderRedundant('external_idp', 'BuilderID')).toBe(false)
  })

  it('无 provider 时无需渲染', () => {
    expect(isProviderRedundant('idc', null)).toBe(true)
    expect(isProviderRedundant('social', undefined)).toBe(true)
  })
})

describe('shouldShowEndpointBadge', () => {
  it('等于默认 endpoint 时省略', () => {
    expect(shouldShowEndpointBadge('ide', 'ide')).toBe(false)
  })

  it('不等于默认 endpoint 时渲染', () => {
    expect(shouldShowEndpointBadge('custom', 'ide')).toBe(true)
  })

  it('默认 endpoint 不可用时保守渲染', () => {
    expect(shouldShowEndpointBadge('ide', undefined)).toBe(true)
    expect(shouldShowEndpointBadge('ide', null)).toBe(true)
  })

  it('凭据无 endpoint 时不渲染', () => {
    expect(shouldShowEndpointBadge('', 'ide')).toBe(false)
  })
})
