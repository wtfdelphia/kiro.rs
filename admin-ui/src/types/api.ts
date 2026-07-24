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

// 添加凭据请求
export interface AddCredentialRequest {
  refreshToken?: string
  authMethod?: 'social' | 'idc' | 'api_key'
  provider?: string
  profileArn?: string
  clientId?: string
  clientSecret?: string
  priority?: number
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

/** 凭据模型列表（缓存或 live） */
export interface CredentialModelsResponse {
  success: boolean
  models: string[]
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

export interface SuccessSettingsResponse {
  success: boolean
  message: string
}
