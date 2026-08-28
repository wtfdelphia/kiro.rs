import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  getCredentialFacets,
  getCredentials,
  setCredentialDisabled,
  setCredentialPriority,
  resetCredentialFailure,
  forceRefreshToken,
  getCredentialBalance,
  addCredential,
  deleteCredential,
  getLoadBalancingMode,
  setLoadBalancingMode,
} from '@/api/credentials'
import { getEndpointSettings } from '@/api/settings'
import type { AddCredentialRequest, CredentialsQuery } from '@/types/api'

/**
 * 查询一页凭据。`placeholderData` 保留上一次的结果，翻页请求进行中时列表不闪空，
 * 只是内容暂时还是上一页的。
 */
export function useCredentials(query: CredentialsQuery = {}) {
  return useQuery({
    queryKey: ['credentials', query],
    queryFn: () => getCredentials(query),
    refetchInterval: 30000, // 每 30 秒刷新一次
    placeholderData: (prev) => prev,
  })
}

/** 筛选下拉的可选值，取自全集；取值随凭据增删变化，故不设长 staleTime */
export function useCredentialFacets() {
  return useQuery({
    queryKey: ['credentialFacets'],
    queryFn: getCredentialFacets,
  })
}

// 查询凭据余额
export function useCredentialBalance(id: number | null) {
  return useQuery({
    queryKey: ['credential-balance', id],
    queryFn: () => getCredentialBalance(id!),
    enabled: id !== null,
    retry: false, // 余额查询失败时不重试（避免重复请求被封禁的账号）
  })
}

// 设置禁用状态
export function useSetDisabled() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, disabled }: { id: number; disabled: boolean }) =>
      setCredentialDisabled(id, disabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 设置优先级
export function useSetPriority() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, priority }: { id: number; priority: number }) =>
      setCredentialPriority(id, priority),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 重置失败计数
export function useResetFailure() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => resetCredentialFailure(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 强制刷新 Token
export function useForceRefreshToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => forceRefreshToken(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 添加新凭据
export function useAddCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: AddCredentialRequest) => addCredential(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 删除凭据
export function useDeleteCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteCredential(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 获取负载均衡模式
export function useLoadBalancingMode() {
  return useQuery({
    queryKey: ['loadBalancingMode'],
    queryFn: getLoadBalancingMode,
  })
}

// 设置负载均衡模式
export function useSetLoadBalancingMode() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setLoadBalancingMode,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['loadBalancingMode'] })
    },
  })
}

/**
 * 当前生效的默认 endpoint，供卡片判断是否渲染 endpoint 徽章。
 * 每张卡片各调一次，靠 queryKey 去重；取值几乎不变，故长 staleTime。
 * 请求失败时返回 undefined，调用方据此保守渲染徽章。
 */
export function useDefaultEndpoint() {
  const { data } = useQuery({
    queryKey: ['endpointSettings'],
    queryFn: getEndpointSettings,
    staleTime: 5 * 60 * 1000,
    retry: false,
  })
  return data?.defaultEndpoint
}
