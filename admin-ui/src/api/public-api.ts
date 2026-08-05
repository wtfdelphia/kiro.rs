import axios from 'axios'
import { storage } from '@/lib/storage'
import type { PublicApiResponse } from '@/types/api'

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

/** 读取对外 Public API 目录（只读） */
export async function getPublicApi(): Promise<PublicApiResponse> {
  const { data } = await api.get<PublicApiResponse>('/public-api')
  return data
}
