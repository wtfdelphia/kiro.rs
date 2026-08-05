// 凭据状态响应
export interface CredentialsStatusResponse {
  total: number
  available: number
  currentId: number
  credentials: CredentialStatusItem[]
}

// 单个凭据状态
export interface CredentialStatusItem {
  id: number
  priority: number
  disabled: boolean
  failureCount: number
  isCurrent: boolean
  expiresAt: string | null
  authMethod: string | null
  hasProfileArn: boolean
  provider?: string | null
  email?: string
  userId?: string | null
  nickname?: string | null
  refreshTokenHash?: string
  apiKeyHash?: string
  maskedApiKey?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  refreshFailureCount: number
  disabledReason?: string
  endpoint: string
  modelCount?: number
  modelsUpdatedAt?: string | null
  modelsLastError?: string | null
}

// 余额响应
export interface BalanceResponse {
  id: number
  subscriptionTitle: string | null
  currentUsage: number
  usageLimit: number
  remaining: number
  usagePercentage: number
  nextResetAt: number | null
}

// 成功响应
export interface SuccessResponse {
  success: boolean
  message: string
}

// 错误响应
export interface AdminErrorResponse {
  error: {
    type: string
    message: string
  }
}

// 请求类型
export interface SetDisabledRequest {
  disabled: boolean
}

export interface SetPriorityRequest {
  priority: number
}

// 认证方式（与后端规范化取值一致）
export type AuthMethod = 'social' | 'idc' | 'external_idp' | 'api_key'

// 添加凭据请求
export interface AddCredentialRequest {
  refreshToken?: string
  authMethod?: AuthMethod
  provider?: string
  profileArn?: string
  clientId?: string
  clientSecret?: string
  priority?: number
  region?: string
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
  kiroApiKey?: string
  endpoint?: string
  userId?: string
  email?: string
  nickname?: string
  startUrl?: string
  onConflict?: 'reject' | 'upsert' | 'replace_token_only'
  /** external_idp 的 OAuth2 token 端点 */
  tokenEndpoint?: string
  /** external_idp 的 issuer URL（未给 tokenEndpoint 时据此派生） */
  issuerUrl?: string
  /** external_idp 的 OAuth2 scopes，空格分隔 */
  scopes?: string
}

// 添加凭据响应
export interface AddCredentialResponse {
  success: boolean
  message: string
  credentialId: number
  email?: string
  action?: 'created' | 'updated'
  userId?: string
}

// ============ 模型目录 / 凭据测试 ============

/** 单凭据模型刷新响应 */
export interface ModelsRefreshResponse {
  success: boolean
  credentialId: number
  count: number
  models: string[]
  updatedAt: string
}

export interface ModelsRefreshErrorItem {
  credentialId: number
  error: string
}

/** 全量模型刷新响应 */
export interface ModelsRefreshAllResponse {
  success: boolean
  refreshed: number
  failed: number
  globalCount: number
  errors: ModelsRefreshErrorItem[]
}

export interface ModelCatalogItem {
  id: string
  resolvable: boolean
  resolveTo?: string | null
  resolveKind?: string | null
  testable: boolean
}

/** 凭据模型列表（缓存或 live） */
export interface CredentialModelsResponse {
  success: boolean
  models: string[]
  modelItems?: ModelCatalogItem[]
  updatedAt?: string | null
  lastError?: string | null
}

/** 凭据推理探测请求 */
export interface TestCredentialRequest {
  model?: string
}

/** 凭据推理探测响应 */
export interface TestCredentialResponse {
  success: boolean
  model: string
  resolvedModel?: string | null
  resolveKind?: string | null
  reply?: string | null
  latencyMs: number
}


// Runtime settings
export interface ProxySettings {
  proxyUrl: string | null
  hasProxyAuth: boolean
  proxyUsername?: string | null
}

export interface EndpointSettings {
  defaultEndpoint: string
  registeredEndpoints: string[]
}

export interface AuthSettings {
  requireApiKey: boolean
  hasApiKey: boolean
  apiKeyMask: string | null
}

export interface ClientIdentitySettings {
  kiroVersion: string
  systemVersion: string
  nodeVersion: string
}

export interface WebSearchSettings {
  webSearchEmulation: boolean
}

export interface SuccessSettingsResponse {
  success: boolean
  message: string
}

// Public API catalog
// 注意：这里是「客户端 -> 本代理」的对外端点，与 EndpointSettings
// （本代理 -> 上游 Kiro 的 ide 端点）是两个不同概念。
export type PublicEndpointStatus = 'live' | 'beta' | 'planned'

export interface PublicApiServerSummary {
  listenHost: string
  port: number
  requireApiKey: boolean
  apiKeyMask: string | null
  hasApiKey: boolean
  authHeaders: string[]
  /** 未配置 publicBaseUrl 时为 null，前端回落 window.location.origin */
  suggestedBaseUrl: string | null
}

export interface PublicApiEndpoint {
  id: string
  method: string
  path: string
  aliases: string[]
  auth: string
  status: PublicEndpointStatus
  stream: boolean
  summary: string
  clientHints: string[]
  examples: { curl: string }
}

export interface PublicApiFamily {
  family: string
  label: string
  endpoints: PublicApiEndpoint[]
}

export interface PublicApiResponse {
  server: PublicApiServerSummary
  families: PublicApiFamily[]
}
