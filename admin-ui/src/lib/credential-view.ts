/** 凭据卡片的展示派生逻辑（可单测，不依赖 React） */

import type { BalanceResponse, CredentialBalanceSnapshot } from '@/types/api'

/**
 * 身份名取值链：`email` → `nickname` → `userId`，全缺时「未获取身份」。
 * id 不在此处拼接，卡片单独渲染 `#<id>` 以免参与截断。
 */
export function resolveIdentityName(credential: {
  email?: string | null
  nickname?: string | null
  userId?: string | null
}): string {
  return credential.email || credential.nickname || credential.userId || '未获取身份'
}

/** `authMethod` 四个取值的可读文案，未知取值回落裸值 */
export function formatAuthMethod(authMethod: string): string {
  switch (authMethod) {
    case 'api_key':
      return 'API Key'
    case 'idc':
      return 'IdC'
    case 'social':
      return 'Social'
    case 'external_idp':
      return '外部 IdP'
    default:
      return authMethod
  }
}

/** `authMethod=idc` 时可推导出的 provider 取值（大小写与连字符不敏感） */
const IDC_PROVIDERS = new Set(['builderid', 'builder-id', 'iam', 'awsbuilderid'])

/**
 * provider 是否可由 `authMethod` 推导，可推导则省略该徽章。
 * `external_idp` 的 provider 承载真实 IdP 信息，恒判定为不可推导。
 */
export function isProviderRedundant(
  authMethod: string | null | undefined,
  provider: string | null | undefined,
): boolean {
  if (!provider) return true
  if (authMethod === 'external_idp') return false
  if (authMethod === 'idc') return IDC_PROVIDERS.has(provider.trim().toLowerCase())
  return false
}

/**
 * 是否渲染 endpoint 徽章。默认 endpoint 未知（请求失败或未加载）时保守渲染，
 * 避免因取值缺失误隐藏真实差异。
 */
export function shouldShowEndpointBadge(
  endpoint: string | null | undefined,
  defaultEndpoint: string | null | undefined,
): boolean {
  if (!endpoint) return false
  if (!defaultEndpoint) return true
  return endpoint !== defaultEndpoint
}

/** 余额展示三态：实时查询结果 / 列表内联缓存快照 / 无数据 */
export type BalanceView =
  | { source: 'live'; remaining: number; usageLimit: number; usagePercentage: number }
  | {
      source: 'cached'
      remaining: number
      usageLimit: number
      usagePercentage: number
      ageSecs: number
      stale: boolean
    }
  | { source: 'none' }

/**
 * 实时结果优先于内联快照。两者都缺时为 `none`，UI 显示「未查询」而非「未知」。
 * 实时结果覆盖缓存后不再带新鲜度信息，标注随之消失。
 */
export function resolveBalanceView(
  live: BalanceResponse | null | undefined,
  snapshot: CredentialBalanceSnapshot | null | undefined,
): BalanceView {
  if (live) {
    return {
      source: 'live',
      remaining: live.remaining,
      usageLimit: live.usageLimit,
      usagePercentage: live.usagePercentage,
    }
  }
  if (snapshot) {
    return {
      source: 'cached',
      remaining: snapshot.remaining,
      usageLimit: snapshot.usageLimit,
      usagePercentage: snapshot.usagePercentage,
      ageSecs: snapshot.ageSecs,
      stale: snapshot.stale,
    }
  }
  return { source: 'none' }
}

/** 缓存年龄的相对时间文案 */
export function formatCacheAge(ageSecs: number): string {
  const secs = Math.max(0, Math.floor(ageSecs))
  if (secs < 60) return `${secs} 秒前`
  const minutes = Math.floor(secs / 60)
  if (minutes < 60) return `${minutes} 分钟前`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} 小时前`
  return `${Math.floor(hours / 24)} 天前`
}

/**
 * 订阅等级三级取值链：本次会话的实时查询 → 列表项顶层 `subscriptionTitle` → 快照内的值。
 * 三者全缺返回 null，由调用方渲染「未知等级」。
 */
export function resolveSubscriptionTitle(
  live: BalanceResponse | null | undefined,
  topLevel: string | null | undefined,
  snapshotTitle: string | null | undefined,
): string | null {
  return live?.subscriptionTitle || topLevel || snapshotTitle || null
}
