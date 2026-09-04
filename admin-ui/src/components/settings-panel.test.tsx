import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { SettingsPanel } from './settings-panel'

const api = vi.hoisted(() => ({
  getProxySettings: vi.fn(),
  getEndpointSettings: vi.fn(),
  getAuthSettings: vi.fn(),
  getClientIdentitySettings: vi.fn(),
  getWebSearchSettings: vi.fn(),
  getWebsocketSettings: vi.fn(),
  updateProxySettings: vi.fn(),
  updateEndpointSettings: vi.fn(),
  updateAuthSettings: vi.fn(),
  updateClientIdentitySettings: vi.fn(),
  updateWebSearchSettings: vi.fn(),
  updateWebsocketSettings: vi.fn(),
}))
vi.mock('@/api/settings', () => api)

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  Toaster: () => null,
}))

beforeEach(() => {
  api.getProxySettings.mockResolvedValue({ proxyUrl: null, proxyUsername: null, hasProxyAuth: false })
  api.getEndpointSettings.mockResolvedValue({ defaultEndpoint: 'ide', registeredEndpoints: ['ide'] })
  api.getAuthSettings.mockResolvedValue({ requireApiKey: true, hasApiKey: true, apiKeyMask: 'sk-****' })
  api.getClientIdentitySettings.mockResolvedValue({
    kiroVersion: '1.0.0',
    systemVersion: 'linux',
    nodeVersion: '20',
  })
  api.getWebSearchSettings.mockResolvedValue({ webSearchEmulation: false })
  api.getWebsocketSettings.mockResolvedValue({
    enabled: true,
    mode: 'http_bridge',
    maxConnections: 64,
    clientFirstMessageTimeoutSeconds: 30,
    interTurnIdleTimeoutSeconds: 0,
    maxMessageBytes: 1_048_576,
    upstreamReadTimeoutSeconds: 60,
    activeConnections: 0,
  })
})

/**
 * 超时参数网格里装的是带文本标签的 label。固定两列时列宽不足以容纳
 * 「turn 间空闲超时（秒，0=不启用）」的自然宽度，窄屏下标签折行且与相邻列错位。
 * 断点取 sm 对齐同文件已有的 grid-cols-1 sm:grid-cols-3 写法。
 */
describe('SettingsPanel WebSocket 超时网格', () => {
  it('窄屏降为单列，sm 及以上恢复两列', async () => {
    render(<SettingsPanel open onOpenChange={vi.fn()} />)

    const label = await screen.findByText('turn 间空闲超时（秒，0=不启用）')
    const tokens = label.closest('label')!.parentElement!.className.split(' ')
    expect(tokens).toContain('grid-cols-1')
    expect(tokens).toContain('sm:grid-cols-2')
    expect(tokens).not.toContain('grid-cols-2')
  })
})
