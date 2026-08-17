/** WebSocket 设置面板的换算与校验纯函数（可单测，不依赖 React） */

export const BYTES_PER_MB = 1024 * 1024

/** 字节 -> MB 显示串；非整 MB 保留两位小数（API 直设的非整 MB 值会在下次保存时取整） */
export function bytesToMb(bytes: number): string {
  const mb = bytes / BYTES_PER_MB
  return Number.isInteger(mb) ? String(mb) : String(Math.round(mb * 100) / 100)
}

/** MB -> 字节（四舍五入到整字节） */
export function mbToBytes(mb: number): number {
  return Math.round(mb * BYTES_PER_MB)
}

export interface WsNumericFields {
  maxConnections: number
  clientFirstMessageTimeoutSeconds: number
  interTurnIdleTimeoutSeconds: number
  maxMessageBytes: number
  upstreamReadTimeoutSeconds: number
}

export interface WsRawInputs {
  maxConnections: string
  clientFirstMessageTimeoutSeconds: string
  interTurnIdleTimeoutSeconds: string
  maxMessageMb: string
  upstreamReadTimeoutSeconds: string
}

type ParseResult = { ok: true; value: WsNumericFields } | { ok: false; error: string }

function parseNonNegativeInt(raw: string, label: string): { ok: true; value: number } | { ok: false; error: string } {
  const trimmed = raw.trim()
  if (trimmed === '') return { ok: false, error: `${label}不能为空` }
  const n = Number(trimmed)
  if (!Number.isInteger(n) || n < 0) {
    return { ok: false, error: `${label}必须是非负整数` }
  }
  return { ok: true, value: n }
}

/** 保存前校验：任一字段非法即整体拦截，返回首个错误 */
export function parseWsNumericFields(input: WsRawInputs): ParseResult {
  const maxConnections = parseNonNegativeInt(input.maxConnections, '最大连接数')
  if (!maxConnections.ok) return maxConnections

  const firstMessage = parseNonNegativeInt(
    input.clientFirstMessageTimeoutSeconds,
    '首帧超时（秒）',
  )
  if (!firstMessage.ok) return firstMessage

  const interTurn = parseNonNegativeInt(input.interTurnIdleTimeoutSeconds, 'turn 间空闲超时（秒）')
  if (!interTurn.ok) return interTurn

  const upstreamRead = parseNonNegativeInt(input.upstreamReadTimeoutSeconds, '上游读超时（秒）')
  if (!upstreamRead.ok) return upstreamRead

  const mbRaw = input.maxMessageMb.trim()
  if (mbRaw === '') return { ok: false, error: '单帧上限不能为空' }
  const mb = Number(mbRaw)
  if (!Number.isFinite(mb) || mb < 1) {
    return { ok: false, error: '单帧上限（MB）必须 ≥ 1' }
  }

  return {
    ok: true,
    value: {
      maxConnections: maxConnections.value,
      clientFirstMessageTimeoutSeconds: firstMessage.value,
      interTurnIdleTimeoutSeconds: interTurn.value,
      maxMessageBytes: mbToBytes(mb),
      upstreamReadTimeoutSeconds: upstreamRead.value,
    },
  }
}
