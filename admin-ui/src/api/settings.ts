import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  AuthSettings,
  ClientIdentitySettings,
  EndpointSettings,
  ProxySettings,
  SuccessSettingsResponse,
} from '@/types/api'

const api = axios.create({
  baseURL: '/api/admin',
  timeout: 30000,
})

api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

export async function getProxySettings(): Promise<ProxySettings> {
  const { data } = await api.get<ProxySettings>('/settings/proxy')
  return data
}

export async function updateProxySettings(body: {
  proxyUrl?: string | null
  proxyUsername?: string | null
  proxyPassword?: string | null
}): Promise<SuccessSettingsResponse> {
  const { data } = await api.put<SuccessSettingsResponse>('/settings/proxy', body)
  return data
}

export async function getEndpointSettings(): Promise<EndpointSettings> {
  const { data } = await api.get<EndpointSettings>('/settings/endpoint')
  return data
}

export async function updateEndpointSettings(body: {
  defaultEndpoint: string
}): Promise<SuccessSettingsResponse> {
  const { data } = await api.put<SuccessSettingsResponse>('/settings/endpoint', body)
  return data
}

export async function getAuthSettings(): Promise<AuthSettings> {
  const { data } = await api.get<AuthSettings>('/settings/auth')
  return data
}

export async function updateAuthSettings(body: {
  requireApiKey?: boolean
  apiKey?: string | null
}): Promise<SuccessSettingsResponse> {
  const { data } = await api.put<SuccessSettingsResponse>('/settings/auth', body)
  return data
}


export async function getClientIdentitySettings(): Promise<ClientIdentitySettings> {
  const { data } = await api.get<ClientIdentitySettings>('/settings/client-identity')
  return data
}

export async function updateClientIdentitySettings(body: {
  kiroVersion?: string
  systemVersion?: string
  nodeVersion?: string
}): Promise<SuccessSettingsResponse> {
  const { data } = await api.put<SuccessSettingsResponse>('/settings/client-identity', body)
  return data
}
