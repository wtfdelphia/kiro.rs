import { useState, useEffect, useMemo, useRef } from 'react'
import { RefreshCw, LogOut, Moon, Sun, Server, Plus, Upload, FileUp, Trash2, RotateCcw, CheckCircle2, KeyRound, Boxes, Settings, Plug, Wallet, Timer, ListChecks, MoreHorizontal } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { storage } from '@/lib/storage'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Select } from '@/components/ui/select'
import { Input } from '@/components/ui/input'
import { CredentialCard } from '@/components/credential-card'
import { BalanceDialog } from '@/components/balance-dialog'
import { AddCredentialDialog } from '@/components/add-credential-dialog'
import { BatchImportDialog } from '@/components/batch-import-dialog'
import { KamImportDialog } from '@/components/kam-import-dialog'
import { OnlineAuthDialog } from '@/components/online-auth-dialog'
import { BatchVerifyDialog, type VerifyResult } from '@/components/batch-verify-dialog'
import { CredentialFilterBar } from '@/components/credential-filter-bar'
import { PaginationBar } from '@/components/pagination-bar'
import { useCredentialFacets, useCredentials, useDeleteCredential, useResetFailure, useLoadBalancingMode, useSetLoadBalancingMode } from '@/hooks/use-credentials'
import { getCredentialBalance, forceRefreshToken, refreshAllModels, fetchAllDisabledIds } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import {
  pageSelectionState,
  toggleCurrentPage,
  toSelection,
  type CredentialSelection,
} from '@/lib/credential-selection'
import {
  queryToSearchParams,
  searchParamsToFilters,
  searchParamsToPage,
  searchParamsToPerPage,
  type CredentialFilters,
} from '@/lib/credential-query-url'
import type { BalanceResponse, ModelsRefreshAllResponse } from '@/types/api'
import { ModelsRefreshResultDialog } from '@/components/models-refresh-result-dialog'
import { SettingsPanel } from '@/components/settings-panel'
import { PublicApiPanel } from '@/components/public-api-panel'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'

interface DashboardProps {
  onLogout: () => void
}

// 定时刷新间隔（秒）。后端余额缓存 TTL 固定 300s，故每轮必须带 force 才能取到新数据
const AUTO_REFRESH_DEFAULT_SECS = 120
const AUTO_REFRESH_MIN_SECS = 10
const AUTO_REFRESH_PRESETS = [30, 60, 120, 180, 300]

export function Dashboard({ onLogout }: DashboardProps) {
  const [selectedCredentialId, setSelectedCredentialId] = useState<number | null>(null)
  const [balanceDialogOpen, setBalanceDialogOpen] = useState(false)
  const [addDialogOpen, setAddDialogOpen] = useState(false)
  const [batchImportDialogOpen, setBatchImportDialogOpen] = useState(false)
  const [kamImportDialogOpen, setKamImportDialogOpen] = useState(false)
  const [onlineAuthDialogOpen, setOnlineAuthDialogOpen] = useState(false)
  // 选中项携带勾选那一刻的属性快照，跨页仍可判断禁用状态与失败次数
  const [selected, setSelected] = useState<Map<number, CredentialSelection>>(new Map())
  const [verifyDialogOpen, setVerifyDialogOpen] = useState(false)
  const [verifying, setVerifying] = useState(false)
  const [verifyProgress, setVerifyProgress] = useState({ current: 0, total: 0 })
  const [verifyResults, setVerifyResults] = useState<Map<number, VerifyResult>>(new Map())
  const [balanceMap, setBalanceMap] = useState<Map<number, BalanceResponse>>(new Map())
  const [loadingBalanceIds, setLoadingBalanceIds] = useState<Set<number>>(new Set())
  const [queryingInfo, setQueryingInfo] = useState(false)
  const [queryInfoProgress, setQueryInfoProgress] = useState({ current: 0, total: 0 })
  // 定时批量余额刷新：默认关闭（每轮对当前页每个启用凭据打一次上游）
  const [autoRefreshEnabled, setAutoRefreshEnabled] = useState(false)
  const [autoRefreshInterval, setAutoRefreshInterval] = useState(AUTO_REFRESH_DEFAULT_SECS)
  const [autoRefreshInput, setAutoRefreshInput] = useState(String(AUTO_REFRESH_DEFAULT_SECS))
  const autoRefreshRunningRef = useRef(false)
  const [batchRefreshing, setBatchRefreshing] = useState(false)
  const [batchRefreshProgress, setBatchRefreshProgress] = useState({ current: 0, total: 0 })
  const [refreshingAllModels, setRefreshingAllModels] = useState(false)
  const [modelsRefreshResultOpen, setModelsRefreshResultOpen] = useState(false)
  const [modelsRefreshResult, setModelsRefreshResult] = useState<ModelsRefreshAllResponse | null>(null)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [publicApiOpen, setPublicApiOpen] = useState(false)
  const cancelVerifyRef = useRef(false)
  const [clearingAll, setClearingAll] = useState(false)
  // 筛选与分页状态从 URL 查询串恢复，每页条数缺省时回落本地存储
  const [filters, setFilters] = useState<CredentialFilters>(() =>
    searchParamsToFilters(window.location.search)
  )
  const [page, setPage] = useState(() => searchParamsToPage(window.location.search))
  const [perPage, setPerPage] = useState(
    () => searchParamsToPerPage(window.location.search) ?? storage.getPerPage()
  )
  const [darkMode, setDarkMode] = useState(() => {
    if (typeof window !== 'undefined') {
      return document.documentElement.classList.contains('dark')
    }
    return false
  })

  const queryClient = useQueryClient()
  const query = useMemo(() => ({ ...filters, page, perPage }), [filters, page, perPage])
  const { data, isLoading, isFetching, error, refetch } = useCredentials(query)
  const { data: facets } = useCredentialFacets()
  const { mutate: deleteCredential } = useDeleteCredential()
  const { mutate: resetFailure } = useResetFailure()
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()

  // 服务端已切页，这里拿到的就是当前页
  const currentCredentials = data?.credentials ?? []
  const pageInfo = data?.pageInfo
  // 全量已禁用数：两个基数都不受筛选与分页影响，相减即得，不必额外请求
  const disabledCredentialCount = data ? data.total - data.available : 0
  const selectedDisabledCount = Array.from(selected.values()).filter(item => item.disabled).length
  const pageSelection = pageSelectionState(
    currentCredentials.map(credential => credential.id),
    selected
  )

  // 筛选与分页状态同步到 URL：用 replaceState，防抖后的连续筛选不该塞满历史栈
  useEffect(() => {
    const search = queryToSearchParams(filters, page, perPage)
    window.history.replaceState(null, '', `${window.location.pathname}?${search}`)
  }, [filters, page, perPage])

  // 总页数因筛选收窄而减少时页码会越界，自动回退末页而不是停在空白页
  useEffect(() => {
    if (!pageInfo) return
    if (pageInfo.totalPages > 0 && page > pageInfo.totalPages) {
      setPage(pageInfo.totalPages)
    }
  }, [pageInfo, page])

  // 清理判据是「已从系统删除」而不是「不在当前页」：分页后两者不再等价，
  // 按后者清理等于每次翻页都丢掉已查到的实时值。
  const dropCachedBalances = (ids: number[]) => {
    if (ids.length === 0) return
    const dropped = new Set(ids)
    setBalanceMap(prev => {
      const next = new Map(prev)
      dropped.forEach(id => next.delete(id))
      return next.size === prev.size ? prev : next
    })
    setLoadingBalanceIds(prev => {
      const next = new Set(prev)
      dropped.forEach(id => next.delete(id))
      return next.size === prev.size ? prev : next
    })
    setSelected(prev => {
      const next = new Map(prev)
      dropped.forEach(id => next.delete(id))
      return next.size === prev.size ? prev : next
    })
  }

  const toggleDarkMode = () => {
    setDarkMode(!darkMode)
    document.documentElement.classList.toggle('dark')
  }

  const handleViewBalance = (id: number) => {
    setSelectedCredentialId(id)
    setBalanceDialogOpen(true)
  }

  // 余额数据的唯一写入口：单卡刷新、批量查询、定时刷新都汇聚到这里
  const applyBalance = (id: number, balance: BalanceResponse) => {
    setBalanceMap(prev => {
      const next = new Map(prev)
      next.set(id, balance)
      return next
    })
  }

  const handleRefresh = () => {
    refetch()
    toast.success('已刷新凭据列表')
  }

  const handleLogout = () => {
    storage.removeApiKey()
    queryClient.clear()
    onLogout()
  }

  // 选择管理：勾选时把判断所需属性一起存下来
  const toggleSelect = (id: number) => {
    const credential = currentCredentials.find(c => c.id === id)
    setSelected(prev => {
      const next = new Map(prev)
      if (next.has(id)) {
        next.delete(id)
      } else if (credential) {
        next.set(id, toSelection(credential))
      }
      return next
    })
  }

  // 全选只作用于当前页；已全选时再点只取消本页，其他页的已选项不动
  const toggleSelectCurrentPage = () => {
    setSelected(prev => toggleCurrentPage(currentCredentials, prev))
  }

  const deselectAll = () => {
    setSelected(new Map())
  }

  // 改任一筛选条件都回到第 1 页：留在原页码大概率直接越界
  const handleFiltersChange = (next: CredentialFilters) => {
    setFilters(next)
    setPage(1)
  }

  const handlePerPageChange = (next: number) => {
    setPerPage(next)
    storage.setPerPage(next)
    setPage(1)
  }

  // 逐个删除，顺带记下真正删成功的 id 供缓存清理使用
  const deleteSequentially = async (ids: number[]) => {
    let successCount = 0
    let failCount = 0
    const deletedIds: number[] = []

    for (const id of ids) {
      try {
        await new Promise<void>((resolve, reject) => {
          deleteCredential(id, {
            onSuccess: () => {
              successCount++
              deletedIds.push(id)
              resolve()
            },
            onError: (err) => {
              failCount++
              reject(err)
            }
          })
        })
      } catch {
        // 错误已在 onError 中处理
      }
    }

    return { successCount, failCount, deletedIds }
  }

  // 批量删除（仅删除已禁用项）
  const handleBatchDelete = async () => {
    if (selected.size === 0) {
      toast.error('请先选择要删除的凭据')
      return
    }

    const disabledIds = Array.from(selected.values())
      .filter(item => item.disabled)
      .map(item => item.id)

    if (disabledIds.length === 0) {
      toast.error('选中的凭据中没有已禁用项')
      return
    }

    const skippedCount = selected.size - disabledIds.length
    const skippedText = skippedCount > 0 ? `（将跳过 ${skippedCount} 个未禁用凭据）` : ''

    if (!confirm(`确定要删除 ${disabledIds.length} 个已禁用凭据吗？此操作无法撤销。${skippedText}`)) {
      return
    }

    const { successCount, failCount, deletedIds } = await deleteSequentially(disabledIds)
    dropCachedBalances(deletedIds)

    const skippedResultText = skippedCount > 0 ? `，已跳过 ${skippedCount} 个未禁用凭据` : ''

    if (failCount === 0) {
      toast.success(`成功删除 ${successCount} 个已禁用凭据${skippedResultText}`)
    } else {
      toast.warning(`删除已禁用凭据：成功 ${successCount} 个，失败 ${failCount} 个${skippedResultText}`)
    }

    deselectAll()
  }

  // 批量恢复异常
  const handleBatchResetFailure = async () => {
    if (selected.size === 0) {
      toast.error('请先选择要恢复的凭据')
      return
    }

    const failedIds = Array.from(selected.values())
      .filter(item => item.failureCount > 0)
      .map(item => item.id)

    if (failedIds.length === 0) {
      toast.error('选中的凭据中没有失败的凭据')
      return
    }

    let successCount = 0
    let failCount = 0

    for (const id of failedIds) {
      try {
        await new Promise<void>((resolve, reject) => {
          resetFailure(id, {
            onSuccess: () => {
              successCount++
              resolve()
            },
            onError: (err) => {
              failCount++
              reject(err)
            }
          })
        })
      } catch (error) {
        // 错误已在 onError 中处理
      }
    }

    if (failCount === 0) {
      toast.success(`成功恢复 ${successCount} 个凭据`)
    } else {
      toast.warning(`成功 ${successCount} 个，失败 ${failCount} 个`)
    }

    deselectAll()
  }

  // 批量刷新 Token
  const handleBatchForceRefresh = async () => {
    if (selected.size === 0) {
      toast.error('请先选择要刷新的凭据')
      return
    }

    const enabledIds = Array.from(selected.values())
      .filter(item => !item.disabled)
      .map(item => item.id)

    if (enabledIds.length === 0) {
      toast.error('选中的凭据中没有启用的凭据')
      return
    }

    setBatchRefreshing(true)
    setBatchRefreshProgress({ current: 0, total: enabledIds.length })

    let successCount = 0
    let failCount = 0

    for (let i = 0; i < enabledIds.length; i++) {
      try {
        await forceRefreshToken(enabledIds[i])
        successCount++
      } catch {
        failCount++
      }
      setBatchRefreshProgress({ current: i + 1, total: enabledIds.length })
    }

    setBatchRefreshing(false)
    queryClient.invalidateQueries({ queryKey: ['credentials'] })

    if (failCount === 0) {
      toast.success(`成功刷新 ${successCount} 个凭据的 Token`)
    } else {
      toast.warning(`刷新 Token：成功 ${successCount} 个，失败 ${failCount} 个`)
    }

    deselectAll()
  }

  /**
   * 一键清除所有已禁用凭据。
   *
   * 范围是全量而非当前页：计数用 `total - available`，待删 id 用带
   * `disabled=true` 的分页查询逐页取回。从 `data.credentials` 过滤会
   * 让「清除所有」静默退化成「清除本页」。
   */
  const handleClearAll = async () => {
    if (disabledCredentialCount === 0) {
      toast.error('没有可清除的已禁用凭据')
      return
    }

    if (!confirm(`确定要清除所有 ${disabledCredentialCount} 个已禁用凭据吗？此操作无法撤销。`)) {
      return
    }

    setClearingAll(true)
    let ids: number[]
    try {
      ids = await fetchAllDisabledIds()
    } catch (error) {
      setClearingAll(false)
      toast.error(`获取已禁用凭据列表失败: ${extractErrorMessage(error)}`)
      return
    }

    const { successCount, failCount, deletedIds } = await deleteSequentially(ids)
    setClearingAll(false)
    dropCachedBalances(deletedIds)

    if (failCount === 0) {
      toast.success(`成功清除所有 ${successCount} 个已禁用凭据`)
    } else {
      toast.warning(`清除已禁用凭据：成功 ${successCount} 个，失败 ${failCount} 个`)
    }

    deselectAll()
  }

  // 查询当前页凭据信息（逐个查询，避免瞬时并发）
  const handleQueryCurrentPageInfo = async () => {
    if (currentCredentials.length === 0) {
      toast.error('当前页没有可查询的凭据')
      return
    }

    const ids = currentCredentials
      .filter(credential => !credential.disabled)
      .map(credential => credential.id)

    if (ids.length === 0) {
      toast.error('当前页没有可查询的启用凭据')
      return
    }

    setQueryingInfo(true)
    setQueryInfoProgress({ current: 0, total: ids.length })

    let successCount = 0
    let failCount = 0

    for (let i = 0; i < ids.length; i++) {
      const id = ids[i]

      setLoadingBalanceIds(prev => {
        const next = new Set(prev)
        next.add(id)
        return next
      })

      try {
        const balance = await getCredentialBalance(id)
        successCount++
        applyBalance(id, balance)
      } catch (error) {
        failCount++
      } finally {
        setLoadingBalanceIds(prev => {
          const next = new Set(prev)
          next.delete(id)
          return next
        })
      }

      setQueryInfoProgress({ current: i + 1, total: ids.length })
    }

    setQueryingInfo(false)

    if (failCount === 0) {
      toast.success(`查询完成：成功 ${successCount}/${ids.length}`)
    } else {
      toast.warning(`查询完成：成功 ${successCount} 个，失败 ${failCount} 个`)
    }
  }

  // 定时刷新一轮：当前页启用凭据逐个 force 刷新，静默成功、失败轮末汇总
  const runAutoRefresh = async () => {
    // 防重入：上一轮未跑完就跳过本轮，不排队堆叠
    if (autoRefreshRunningRef.current) return
    // 手动批量操作优先，避免与其争抢上游
    if (queryingInfo || verifying || batchRefreshing) return

    const ids = currentCredentials
      .filter(credential => !credential.disabled)
      .map(credential => credential.id)

    // 无启用凭据时静默跳过（周期性错误提示无操作价值）
    if (ids.length === 0) return

    autoRefreshRunningRef.current = true
    let failCount = 0

    try {
      for (const id of ids) {
        setLoadingBalanceIds(prev => {
          const next = new Set(prev)
          next.add(id)
          return next
        })

        try {
          const balance = await getCredentialBalance(id, true)
          applyBalance(id, balance)
        } catch {
          failCount++
        } finally {
          setLoadingBalanceIds(prev => {
            const next = new Set(prev)
            next.delete(id)
            return next
          })
        }
      }
    } finally {
      autoRefreshRunningRef.current = false
    }

    if (failCount > 0) {
      toast.warning(`定时刷新：${failCount}/${ids.length} 个凭据失败`)
    }
  }

  // 执行体经 ref 持有最新闭包，使 timer 的依赖只有开关与间隔，
  // 避免把 currentCredentials 放进依赖导致每次 render 重建 interval
  const autoRefreshTaskRef = useRef(runAutoRefresh)
  autoRefreshTaskRef.current = runAutoRefresh

  useEffect(() => {
    if (!autoRefreshEnabled) return
    const timer = setInterval(() => {
      void autoRefreshTaskRef.current()
    }, autoRefreshInterval * 1000)
    return () => clearInterval(timer)
  }, [autoRefreshEnabled, autoRefreshInterval])

  // 手动输入间隔：正整数且不低于下界，非法值保留原值
  const commitAutoRefreshInput = () => {
    const parsed = Number(autoRefreshInput)
    if (!Number.isInteger(parsed) || parsed < AUTO_REFRESH_MIN_SECS) {
      toast.error(`间隔需为不小于 ${AUTO_REFRESH_MIN_SECS} 的整数秒`)
      setAutoRefreshInput(String(autoRefreshInterval))
      return
    }
    setAutoRefreshInterval(parsed)
  }

  // 批量验活
  const handleBatchVerify = async () => {
    if (selected.size === 0) {
      toast.error('请先选择要验活的凭据')
      return
    }

    // 初始化状态
    setVerifying(true)
    cancelVerifyRef.current = false
    const ids = Array.from(selected.keys())
    setVerifyProgress({ current: 0, total: ids.length })

    let successCount = 0

    // 初始化结果，所有凭据状态为 pending
    const initialResults = new Map<number, VerifyResult>()
    ids.forEach(id => {
      initialResults.set(id, { id, status: 'pending' })
    })
    setVerifyResults(initialResults)
    setVerifyDialogOpen(true)

    // 开始验活
    for (let i = 0; i < ids.length; i++) {
      // 检查是否取消
      if (cancelVerifyRef.current) {
        toast.info('已取消验活')
        break
      }

      const id = ids[i]

      // 更新当前凭据状态为 verifying
      setVerifyResults(prev => {
        const newResults = new Map(prev)
        newResults.set(id, { id, status: 'verifying' })
        return newResults
      })

      try {
        // 验活必须跳过 TTL 缓存：命中缓存会把窗口内已失效的凭据报成 success
        const balance = await getCredentialBalance(id, true)
        successCount++

        // 更新为成功状态
        setVerifyResults(prev => {
          const newResults = new Map(prev)
          newResults.set(id, {
            id,
            status: 'success',
            usage: `${balance.currentUsage}/${balance.usageLimit}`
          })
          return newResults
        })
      } catch (error) {
        // 更新为失败状态
        setVerifyResults(prev => {
          const newResults = new Map(prev)
          newResults.set(id, {
            id,
            status: 'failed',
            error: extractErrorMessage(error)
          })
          return newResults
        })
      }

      // 更新进度
      setVerifyProgress({ current: i + 1, total: ids.length })

      // 添加延迟防止封号（最后一个不需要延迟）
      if (i < ids.length - 1 && !cancelVerifyRef.current) {
        await new Promise(resolve => setTimeout(resolve, 2000))
      }
    }

    setVerifying(false)

    if (!cancelVerifyRef.current) {
      toast.success(`验活完成：成功 ${successCount}/${ids.length}`)
    }
  }

  // 取消验活
  const handleCancelVerify = () => {
    cancelVerifyRef.current = true
    setVerifying(false)
  }

  // 刷新全部模型目录
  const handleRefreshAllModels = async () => {
    if (refreshingAllModels) return
    setRefreshingAllModels(true)
    try {
      const result = await refreshAllModels()
      setModelsRefreshResult(result)
      const summary = `刷新完成：成功 ${result.refreshed}，失败 ${result.failed}，全局 ${result.globalCount} 个模型`
      if (result.failed > 0) {
        toast.warning(summary)
        setModelsRefreshResultOpen(true)
      } else {
        toast.success(summary)
      }
    } catch (error) {
      toast.error(`刷新全部模型失败: ${extractErrorMessage(error)}`)
    } finally {
      setRefreshingAllModels(false)
    }
  }

  // 切换负载均衡模式
  const handleToggleLoadBalancing = () => {
    const currentMode = loadBalancingData?.mode || 'priority'
    const newMode = currentMode === 'priority' ? 'balanced' : 'priority'

    setLoadBalancingMode(newMode, {
      onSuccess: () => {
        const modeName = newMode === 'priority' ? '优先级模式' : '均衡负载模式'
        toast.success(`已切换到${modeName}`)
      },
      onError: (error) => {
        toast.error(`切换失败: ${extractErrorMessage(error)}`)
      }
    })
  }

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary mx-auto mb-4"></div>
          <p className="text-muted-foreground">加载中...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background p-4">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <div className="text-red-500 mb-4">加载失败</div>
            <p className="text-muted-foreground mb-4">{(error as Error).message}</p>
            <div className="space-x-2">
              <Button onClick={() => refetch()}>重试</Button>
              <Button variant="outline" onClick={handleLogout}>重新登录</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-background">
      {/* 顶部导航 */}
      <header className="sticky top-0 z-50 w-full border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="container flex min-h-14 flex-wrap items-center justify-between gap-y-2 px-4 md:px-8">
          <div className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            <span className="font-semibold">Kiro Admin</span>
          </div>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleToggleLoadBalancing}
              disabled={isLoadingMode || isSettingMode}
              title="切换负载均衡模式"
            >
              {isLoadingMode ? '加载中...' : (loadBalancingData?.mode === 'priority' ? '优先级模式' : '均衡负载')}
            </Button>
            <Button variant="ghost" size="icon" onClick={toggleDarkMode}>
              {darkMode ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}
            </Button>
            <Button variant="ghost" size="icon" onClick={handleRefresh}>
              <RefreshCw className="h-5 w-5" />
            </Button>
            <Button variant="ghost" size="icon" onClick={() => setPublicApiOpen(true)} title="对外 API 端点">
              <Plug className="h-5 w-5" />
            </Button>
            <Button variant="ghost" size="icon" onClick={() => setSettingsOpen(true)} title="运行时设置">
              <Settings className="h-5 w-5" />
            </Button>
            <Button variant="ghost" size="icon" onClick={handleLogout}>
              <LogOut className="h-5 w-5" />
            </Button>
          </div>
        </div>
      </header>

      {/* 主内容 */}
      <main className="container mx-auto px-4 md:px-8 py-6">
        {/* 统计卡片 */}
        <div className="grid gap-4 md:grid-cols-3 mb-6">
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                凭据总数
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{data?.total || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                可用凭据
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold text-green-600">{data?.available || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                当前活跃
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold flex items-center gap-2">
                #{data?.currentId || '-'}
                <Badge variant="success">活跃</Badge>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* 凭据列表 */}
        <div className="space-y-4">
          {/* 框线对齐筛选栏：标题到「添加凭据」是一整组操作区，圈起来才能跟下方列表分开 */}
          <div className="flex flex-wrap items-center justify-between gap-y-2 rounded-md border p-3">
            {/* 勾选后子项从 2 个涨到 3 个，nowrap 会把标题压成竖排：必须允许折行 */}
            <div className="flex flex-wrap items-center gap-4 gap-y-2">
              {/* leading-9 让标题与同排的 h-9 按钮等高：高度不齐时 items-center 会在行内各自居中，换行后错开 4px */}
              <h2 className="text-xl font-semibold leading-9">凭据管理</h2>
              {currentCredentials.length > 0 && (
                <Button
                  onClick={toggleSelectCurrentPage}
                  size="sm"
                  variant="outline"
                  aria-pressed={pageSelection === 'all'}
                  title="全选范围仅限当前页，其他页的已选项不受影响"
                >
                  <ListChecks className="h-4 w-4 mr-2" />
                  {pageSelection === 'all'
                    ? '取消本页全选'
                    : pageSelection === 'partial'
                      ? '本页部分已选，全选本页'
                      : '全选本页'}
                </Button>
              )}
              {selected.size > 0 && (
                <div className="flex items-center gap-2">
                  {/* N 是跨页累计，单独给出当前页条数才能看清即将执行的范围 */}
                  <Badge variant="secondary">
                    已选 {selected.size} 条 / 当前页 {currentCredentials.length} 条
                  </Badge>
                  <Button onClick={deselectAll} size="sm" variant="ghost">
                    取消选择
                  </Button>
                </div>
              )}
            </div>
            <div className="flex flex-wrap justify-end gap-2">
              {currentCredentials.length > 0 && (
                <div
                  className="flex items-center gap-1.5 rounded-md border px-2 py-1"
                  title="每隔指定秒数强制刷新当前页启用凭据的余额（跳过缓存）。每轮对每个凭据发起一次上游请求，默认关闭"
                >
                  <Timer className="h-4 w-4 text-muted-foreground shrink-0" />
                  <span className="text-sm whitespace-nowrap">定时刷新</span>
                  <Switch
                    checked={autoRefreshEnabled}
                    onCheckedChange={setAutoRefreshEnabled}
                  />
                  <Select
                    value={String(autoRefreshInterval)}
                    onValueChange={(value) => {
                      setAutoRefreshInterval(Number(value))
                      setAutoRefreshInput(value)
                    }}
                    options={AUTO_REFRESH_PRESETS.map(secs => ({
                      value: String(secs),
                      label: `${secs}s`,
                    }))}
                    triggerClassName="h-7 w-20 px-2 text-sm"
                  />
                  <Input
                    className="h-7 w-16 text-sm"
                    value={autoRefreshInput}
                    onChange={(e) => setAutoRefreshInput(e.target.value)}
                    onBlur={commitAutoRefreshInput}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') commitAutoRefreshInput()
                    }}
                    aria-label="自定义刷新间隔（秒）"
                    title={`自定义间隔，单位秒，不小于 ${AUTO_REFRESH_MIN_SECS}`}
                  />
                </div>
              )}
              <Button onClick={() => setAddDialogOpen(true)} size="sm">
                <Plus className="h-4 w-4 mr-2" />
                添加凭据
              </Button>
              {/* 低频操作收进溢出菜单：选中态右组控件从 12 个降到 8 个，
                  宽度需求降到容器上限以内，标题行不再被折行吃掉两行高度 */}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" variant="outline" aria-label="更多操作" title="导入与维护类操作">
                    <MoreHorizontal className="h-4 w-4 mr-2" />
                    更多操作
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-52">
                  <DropdownMenuItem onSelect={() => setKamImportDialogOpen(true)}>
                    <FileUp />
                    Kiro Account Manager 导入
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => setBatchImportDialogOpen(true)}>
                    <Upload />
                    批量导入
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => setOnlineAuthDialogOpen(true)}>
                    <KeyRound />
                    在线授权
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    onSelect={() => void handleRefreshAllModels()}
                    disabled={refreshingAllModels || !data?.total}
                  >
                    <Boxes className={refreshingAllModels ? 'animate-spin' : ''} />
                    {refreshingAllModels ? '刷新模型中...' : '刷新全部模型'}
                  </DropdownMenuItem>
                  {/* 全量语义的入口，可用性判据取全量已禁用数，不受当前页有无已禁用影响 */}
                  <DropdownMenuItem
                    onSelect={() => void handleClearAll()}
                    disabled={disabledCredentialCount === 0 || clearingAll}
                    className="text-destructive focus:text-destructive"
                    title={disabledCredentialCount === 0 ? '没有可清除的已禁用凭据' : `清除全部 ${disabledCredentialCount} 个已禁用凭据`}
                  >
                    <Trash2 />
                    {clearingAll ? '清除中...' : `清除已禁用${disabledCredentialCount > 0 ? ` (${disabledCredentialCount})` : ''}`}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
          {/* 批量操作独立成工具栏行：与标题行的全局操作同排时总需求宽度超出
              容器上限，折行混排且层次混乱；拆开后标题行只留全局操作。
              行内有凭据时常驻：未选中时只有批量余额/订阅，选中后追加四项批量操作 */}
          {(currentCredentials.length > 0 || (verifying && !verifyDialogOpen)) && (
            <div role="toolbar" aria-label="批量操作" className="flex flex-wrap items-center gap-2 rounded-md border bg-muted/40 p-2">
              <Button
                onClick={handleQueryCurrentPageInfo}
                size="sm"
                variant="outline"
                disabled={queryingInfo}
                title="查询当前页启用凭据的余额/订阅，可能返回最近 5 分钟内的缓存结果；需要最新数据请用单卡「刷新余额」或开启定时刷新"
              >
                <Wallet className={`h-4 w-4 mr-2 ${queryingInfo ? 'animate-spin' : ''}`} />
                {queryingInfo ? `查询中... ${queryInfoProgress.current}/${queryInfoProgress.total}` : '批量余额/订阅'}
              </Button>
              {selected.size > 0 && (
                <>
              <Button onClick={handleBatchVerify} size="sm" variant="outline">
                <CheckCircle2 className="h-4 w-4 mr-2" />
                批量验活
              </Button>
              <Button
                onClick={handleBatchForceRefresh}
                size="sm"
                variant="outline"
                disabled={batchRefreshing}
              >
                <RefreshCw className={`h-4 w-4 mr-2 ${batchRefreshing ? 'animate-spin' : ''}`} />
                {batchRefreshing ? `刷新中... ${batchRefreshProgress.current}/${batchRefreshProgress.total}` : '批量刷新 Token'}
              </Button>
              <Button onClick={handleBatchResetFailure} size="sm" variant="outline">
                <RotateCcw className="h-4 w-4 mr-2" />
                恢复异常
              </Button>
              <Button
                onClick={handleBatchDelete}
                size="sm"
                variant="destructive"
                disabled={selectedDisabledCount === 0}
                title={selectedDisabledCount === 0 ? '只能删除已禁用凭据' : undefined}
              >
                <Trash2 className="h-4 w-4 mr-2" />
                批量删除
              </Button>
                </>
              )}
              {verifying && !verifyDialogOpen && (
                <Button onClick={() => setVerifyDialogOpen(true)} size="sm" variant="secondary">
                  <CheckCircle2 className="h-4 w-4 mr-2 animate-spin" />
                  验活中... {verifyProgress.current}/{verifyProgress.total}
                </Button>
              )}
            </div>
          )}
          <CredentialFilterBar filters={filters} onChange={handleFiltersChange} facets={facets} />

          {currentCredentials.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">
                {pageInfo && pageInfo.filteredTotal === 0 && data && data.total > 0
                  ? '没有符合筛选条件的凭据'
                  : '暂无凭据'}
              </CardContent>
            </Card>
          ) : (
            <>
              {/* 翻页请求进行中只加一层半透明，不清空列表 */}
              <div
                className={`grid gap-4 md:grid-cols-2 lg:grid-cols-3 [&>*]:min-w-0 ${isFetching ? 'opacity-60 transition-opacity' : ''}`}
                aria-busy={isFetching}
              >
                {currentCredentials.map((credential) => (
                  <CredentialCard
                    key={credential.id}
                    credential={credential}
                    onViewBalance={handleViewBalance}
                    selected={selected.has(credential.id)}
                    onToggleSelect={() => toggleSelect(credential.id)}
                    balance={balanceMap.get(credential.id) || null}
                    loadingBalance={loadingBalanceIds.has(credential.id)}
                    onBalanceRefreshed={applyBalance}
                  />
                ))}
              </div>

              {pageInfo && (
                <PaginationBar
                  pageInfo={pageInfo}
                  onPageChange={setPage}
                  onPerPageChange={handlePerPageChange}
                />
              )}
            </>
          )}
        </div>
      </main>

      {/* 余额对话框 */}
      <BalanceDialog
        credentialId={selectedCredentialId}
        open={balanceDialogOpen}
        onOpenChange={setBalanceDialogOpen}
      />

      {/* 添加凭据对话框 */}
      <AddCredentialDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
      />

      {/* 批量导入对话框 */}
      <BatchImportDialog
        open={batchImportDialogOpen}
        onOpenChange={setBatchImportDialogOpen}
      />

      {/* KAM 账号导入对话框 */}
      <KamImportDialog
        open={kamImportDialogOpen}
        onOpenChange={setKamImportDialogOpen}
      />

      <OnlineAuthDialog
        open={onlineAuthDialogOpen}
        onOpenChange={setOnlineAuthDialogOpen}
      />

      {/* 批量验活对话框 */}
      <BatchVerifyDialog
        open={verifyDialogOpen}
        onOpenChange={setVerifyDialogOpen}
        verifying={verifying}
        progress={verifyProgress}
        results={verifyResults}
        onCancel={handleCancelVerify}
      />

      <ModelsRefreshResultDialog
        open={modelsRefreshResultOpen}
        onOpenChange={setModelsRefreshResultOpen}
        result={modelsRefreshResult}
      />

      <SettingsPanel open={settingsOpen} onOpenChange={setSettingsOpen} />
      <PublicApiPanel open={publicApiOpen} onOpenChange={setPublicApiOpen} />
    </div>
  )
}
