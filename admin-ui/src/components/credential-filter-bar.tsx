import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { formatAuthMethod } from '@/lib/credential-view'
import type { CredentialFilters } from '@/lib/credential-query-url'
import { SUBSCRIPTION_TITLE_UNKNOWN, type CredentialFacetsResponse } from '@/types/api'

/** 文本输入防抖时长（毫秒）：逐字符发请求会把列表接口打成搜索接口 */
const TEXT_DEBOUNCE_MS = 300

/** 下拉「不限」选项的值，空串在 Select 里无法与未选态区分 */
const ANY = '__any__'

interface CredentialFilterBarProps {
  filters: CredentialFilters
  onChange: (next: CredentialFilters) => void
  facets?: CredentialFacetsResponse
}

export function CredentialFilterBar({ filters, onChange, facets }: CredentialFilterBarProps) {
  // 文本类维度先落本地态，防抖后才并入查询
  const [emailInput, setEmailInput] = useState(filters.email ?? '')
  const [idInput, setIdInput] = useState(filters.id ?? '')
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange
  const filtersRef = useRef(filters)
  filtersRef.current = filters

  useEffect(() => {
    const email = emailInput.trim()
    const id = idInput.trim()
    if ((filtersRef.current.email ?? '') === email && (filtersRef.current.id ?? '') === id) return
    const timer = setTimeout(() => {
      onChangeRef.current({
        ...filtersRef.current,
        email: email || undefined,
        id: id || undefined,
      })
    }, TEXT_DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [emailInput, idInput])

  const patch = (next: Partial<CredentialFilters>) => onChange({ ...filters, ...next })

  const parsePriority = (raw: string): number | undefined => {
    const value = Number(raw)
    return raw.trim() !== '' && Number.isInteger(value) && value >= 0 ? value : undefined
  }

  const titleOptions = [
    { value: ANY, label: '全部等级' },
    { value: SUBSCRIPTION_TITLE_UNKNOWN, label: '无订阅等级' },
    ...(facets?.subscriptionTitles ?? []).map((title) => ({ value: title, label: title })),
  ]

  const methodOptions = [
    { value: ANY, label: '全部类型' },
    ...(facets?.authMethods ?? []).map((method) => ({
      value: method,
      label: formatAuthMethod(method),
    })),
  ]

  const hasAny =
    Object.values(filters).some((value) => value !== undefined && value !== '') ||
    emailInput !== '' ||
    idInput !== ''

  const reset = () => {
    setEmailInput('')
    setIdInput('')
    onChange({})
  }

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-md border p-3">
      <Select
        value={filters.subscriptionTitle ?? ANY}
        onValueChange={(value) =>
          patch({ subscriptionTitle: value === ANY ? undefined : value })
        }
        options={titleOptions}
        triggerClassName="h-9 w-36 px-2 text-sm"
      />

      <Select
        value={filters.authMethod ?? ANY}
        onValueChange={(value) => patch({ authMethod: value === ANY ? undefined : value })}
        options={methodOptions}
        triggerClassName="h-9 w-36 px-2 text-sm"
      />

      <Select
        value={filters.disabled === undefined ? ANY : String(filters.disabled)}
        onValueChange={(value) =>
          patch({ disabled: value === ANY ? undefined : value === 'true' })
        }
        options={[
          { value: ANY, label: '全部状态' },
          { value: 'false', label: '仅未禁用' },
          { value: 'true', label: '仅已禁用' },
        ]}
        triggerClassName="h-9 w-32 px-2 text-sm"
      />

      <Select
        value={filters.hasProfileArn === undefined ? ANY : String(filters.hasProfileArn)}
        onValueChange={(value) =>
          patch({ hasProfileArn: value === ANY ? undefined : value === 'true' })
        }
        options={[
          { value: ANY, label: '全部 Profile' },
          { value: 'true', label: '有 Profile ARN' },
          { value: 'false', label: '无 Profile ARN' },
        ]}
        triggerClassName="h-9 w-40 px-2 text-sm"
      />

      <div className="flex items-center gap-1">
        <Input
          className="h-9 w-20 text-sm"
          aria-label="优先级下限"
          placeholder="优先级≥"
          value={filters.priorityMin ?? ''}
          onChange={(e) => patch({ priorityMin: parsePriority(e.target.value) })}
        />
        <span className="text-sm text-muted-foreground">-</span>
        <Input
          className="h-9 w-20 text-sm"
          aria-label="优先级上限"
          placeholder="优先级≤"
          value={filters.priorityMax ?? ''}
          onChange={(e) => patch({ priorityMax: parsePriority(e.target.value) })}
        />
      </div>

      <Input
        className="h-9 w-48 text-sm"
        aria-label="按 email 筛选"
        placeholder="email 包含"
        value={emailInput}
        onChange={(e) => setEmailInput(e.target.value)}
      />

      <Input
        className="h-9 w-28 text-sm"
        aria-label="按 id 筛选"
        placeholder="id 包含"
        value={idInput}
        onChange={(e) => setIdInput(e.target.value)}
      />

      {hasAny && (
        <Button variant="ghost" size="sm" onClick={reset}>
          清空筛选
        </Button>
      )}
    </div>
  )
}
