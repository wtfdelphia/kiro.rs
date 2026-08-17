import { describe, expect, it } from 'vitest'
import {
  BYTES_PER_MB,
  bytesToMb,
  mbToBytes,
  parseWsNumericFields,
  type WsRawInputs,
} from './websocket-settings'

describe('bytesToMb / mbToBytes', () => {
  it('整 MB 往返无损', () => {
    expect(bytesToMb(32 * BYTES_PER_MB)).toBe('32')
    expect(mbToBytes(32)).toBe(32 * BYTES_PER_MB)
  })

  it('非整 MB 显示保留两位小数', () => {
    expect(bytesToMb(1.5 * BYTES_PER_MB)).toBe('1.5')
  })

  it('MB 转字节四舍五入到整字节', () => {
    expect(mbToBytes(1.5)).toBe(1.5 * BYTES_PER_MB)
    expect(mbToBytes(64)).toBe(67108864)
  })
})

const validInputs: WsRawInputs = {
  maxConnections: '64',
  clientFirstMessageTimeoutSeconds: '15',
  interTurnIdleTimeoutSeconds: '1800',
  maxMessageMb: '32',
  upstreamReadTimeoutSeconds: '300',
}

describe('parseWsNumericFields', () => {
  it('合法输入解析为字节单位的字段集', () => {
    const r = parseWsNumericFields(validInputs)
    expect(r.ok).toBe(true)
    if (!r.ok) return
    expect(r.value.maxConnections).toBe(64)
    expect(r.value.maxMessageBytes).toBe(32 * BYTES_PER_MB)
    expect(r.value.interTurnIdleTimeoutSeconds).toBe(1800)
  })

  it('turn 间空闲超时允许 0（表示不启用）', () => {
    const r = parseWsNumericFields({ ...validInputs, interTurnIdleTimeoutSeconds: '0' })
    expect(r.ok).toBe(true)
    if (!r.ok) return
    expect(r.value.interTurnIdleTimeoutSeconds).toBe(0)
  })

  it('负数被拦截', () => {
    const r = parseWsNumericFields({ ...validInputs, maxConnections: '-1' })
    expect(r.ok).toBe(false)
    if (r.ok) return
    expect(r.error).toContain('最大连接数')
  })

  it('非整数被拦截', () => {
    const r = parseWsNumericFields({ ...validInputs, upstreamReadTimeoutSeconds: '1.5' })
    expect(r.ok).toBe(false)
    if (r.ok) return
    expect(r.error).toContain('上游读超时')
  })

  it('空值被拦截', () => {
    const r = parseWsNumericFields({ ...validInputs, clientFirstMessageTimeoutSeconds: '  ' })
    expect(r.ok).toBe(false)
    if (r.ok) return
    expect(r.error).toContain('首帧超时')
  })

  it('单帧上限低于 1 MB 被拦截', () => {
    const r = parseWsNumericFields({ ...validInputs, maxMessageMb: '0.5' })
    expect(r.ok).toBe(false)
    if (r.ok) return
    expect(r.error).toContain('单帧上限')
  })

  it('非数字文本被拦截', () => {
    const r = parseWsNumericFields({ ...validInputs, maxMessageMb: 'abc' })
    expect(r.ok).toBe(false)
  })
})
