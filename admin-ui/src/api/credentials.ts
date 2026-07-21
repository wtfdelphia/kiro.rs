import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  CredentialsStatusResponse,
  BalanceResponse,
  SuccessResponse,
  SetDisabledRequest,
  SetPriorityRequest,
  AddCredentialRequest,
  AddCredentialResponse,
} from '@/types/api'

// 创建 axios 实例
const api = axios.create({
  baseURL: '/api/admin',
  headers: {
    'Content-Type': 'application/json',
  },
})

// 请求拦截器添加 API Key
api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

// 获取所有凭据状态
export async function getCredentials(): Promise<CredentialsStatusResponse> {
  const { data } = await api.get<CredentialsStatusResponse>('/credentials')
  return data
}

// 设置凭据禁用状态
export async function setCredentialDisabled(
  id: number,
  disabled: boolean
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/disabled`,
    { disabled } as SetDisabledRequest
  )
  return data
}

// 设置凭据优先级
export async function setCredentialPriority(
  id: number,
  priority: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/priority`,
    { priority } as SetPriorityRequest
  )
  return data
}

// 重置失败计数
export async function resetCredentialFailure(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset`)
  return data
}

// 强制刷新 Token
export async function forceRefreshToken(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/refresh`)
  return data
}

// 获取凭据余额
export async function getCredentialBalance(id: number): Promise<BalanceResponse> {
  const { data } = await api.get<BalanceResponse>(`/credentials/${id}/balance`)
  return data
}

// 添加新凭据
export async function addCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials', req)
  return data
}

// 导入凭据（默认 upsert）
export async function importCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials/import', req)
  return data
}

// 删除凭据
export async function deleteCredential(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/credentials/${id}`)
  return data
}

// 获取负载均衡模式
export async function getLoadBalancingMode(): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.get<{ mode: 'priority' | 'balanced' }>('/config/load-balancing')
  return data
}

// 设置负载均衡模式
export async function setLoadBalancingMode(mode: 'priority' | 'balanced'): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.put<{ mode: 'priority' | 'balanced' }>('/config/load-balancing', { mode })
  return data
}


export interface BatchImportOptions {
  onConflict?: 'reject' | 'upsert' | 'replace_token_only'
  stopOnError?: boolean
  fetchBalance?: boolean
  concurrency?: number
}

export interface BatchImportRequest {
  items: AddCredentialRequest[]
  options?: BatchImportOptions
}

export interface BatchImportItemResult {
  index: number
  status: string
  credentialId?: number
  email?: string
  userId?: string
  error?: string
  balance?: BalanceResponse
  warning?: string
}

export interface BatchImportResponse {
  success: boolean
  summary: { created: number; updated: number; duplicate: number; failed: number }
  results: BatchImportItemResult[]
}

export async function importCredentialsBatch(
  req: BatchImportRequest
): Promise<BatchImportResponse> {
  const { data } = await api.post<BatchImportResponse>('/credentials/import/batch', req)
  return data
}

// ============ 在线授权 ============

export interface BuilderIdStartResponse {
  sessionId: string
  userCode: string
  verificationUri: string
  interval: number
  expiresIn: number
}

export type BuilderIdPollResponse =
  | {
      success: boolean
      completed: false
      status: string
      interval: number
    }
  | {
      success: boolean
      completed: true
      credentialId: number
      email?: string
      userId?: string
      action?: string
    }

export interface IamSsoStartResponse {
  sessionId: string
  authorizeUrl: string
  expiresIn: number
}

export interface SsoTokenImportResponse {
  success: boolean
  accounts: Array<{ credentialId: number; email?: string; userId?: string }>
  errors?: string[]
}

export async function startBuilderIdLogin(region?: string): Promise<BuilderIdStartResponse> {
  const { data } = await api.post<BuilderIdStartResponse>('/auth/builderid/start', {
    region: region || undefined,
  })
  return data
}

export async function pollBuilderIdLogin(sessionId: string): Promise<BuilderIdPollResponse> {
  const { data } = await api.post<BuilderIdPollResponse>('/auth/builderid/poll', { sessionId })
  return data
}

export async function startIamSsoLogin(
  startUrl: string,
  region?: string
): Promise<IamSsoStartResponse> {
  const { data } = await api.post<IamSsoStartResponse>('/auth/iam-sso/start', {
    startUrl,
    region: region || undefined,
  })
  return data
}

export async function completeIamSsoLogin(
  sessionId: string,
  callbackUrl: string
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/auth/iam-sso/complete', {
    sessionId,
    callbackUrl,
  })
  return data
}

export async function importSsoToken(
  bearerToken: string,
  region?: string
): Promise<SsoTokenImportResponse> {
  const { data } = await api.post<SsoTokenImportResponse>('/auth/sso-token', {
    bearerToken,
    region: region || undefined,
  })
  return data
}
