import { describe, expect, it } from 'vitest'
import {
  describeContainer,
  describePreviewItem,
  parseJsonDocument,
} from './kam-import-dialog'
import type { KamPreviewItem } from '@/api/credentials'

function previewItem(overrides: Partial<KamPreviewItem> = {}): KamPreviewItem {
  return {
    index: 0,
    path: '$[0]',
    hasRefreshToken: true,
    hasClientId: false,
    hasClientSecret: false,
    hasTokenEndpoint: false,
    hasIssuerUrl: false,
    hasScopes: false,
    hasProfileArn: false,
    disabled: false,
    valid: true,
    ...overrides,
  }
}

describe('parseJsonDocument', () => {
  it('原样返回解析后的文档，不做容器判别', () => {
    // 客户端不再判别容器：平铺数组、wrapper、嵌套都只做语法检查
    expect(parseJsonDocument('[{"refreshToken":"x"}]')).toEqual([
      { refreshToken: 'x' },
    ])
    expect(parseJsonDocument('{"version":"1.9.2","accounts":[]}')).toEqual({
      version: '1.9.2',
      accounts: [],
    })
    expect(parseJsonDocument('{"credentials":{"refreshToken":"x"}}')).toEqual({
      credentials: { refreshToken: 'x' },
    })
  })

  it('不因缺少 refreshToken 而拒绝——判别是服务端的职责', () => {
    // 旧实现会在此处本地判失败；现在必须放行到服务端，由其给出逐条原因
    expect(() => parseJsonDocument('{"version":"1.0","data":[]}')).not.toThrow()
    expect(() => parseJsonDocument('[{"label":"no token"}]')).not.toThrow()
  })

  it('不本地推断认证类型', () => {
    // 只有 clientId 无 clientSecret 的 external 公共客户端：客户端不得判失败
    const doc = parseJsonDocument(
      '[{"authMethod":"external_idp","refreshToken":"x","clientId":"cid"}]'
    ) as Array<Record<string, unknown>>
    // 原样透传，authMethod 未被重算
    expect(doc[0].authMethod).toBe('external_idp')
    expect(doc[0].clientSecret).toBeUndefined()
  })

  it('空输入与语法错误抛出可读错误', () => {
    expect(() => parseJsonDocument('')).toThrow(/请输入/)
    expect(() => parseJsonDocument('   ')).toThrow(/请输入/)
    expect(() => parseJsonDocument('{not json}')).toThrow()
  })
})

describe('describePreviewItem', () => {
  it('展示认证类型与字段完整性，不含字段值', () => {
    const text = describePreviewItem(
      previewItem({
        authMethod: 'external_idp',
        hasClientId: true,
        hasTokenEndpoint: true,
        hasScopes: true,
        hasProfileArn: true,
      })
    )
    expect(text).toContain('external_idp')
    expect(text).toContain('clientId')
    expect(text).toContain('tokenEndpoint')
    expect(text).toContain('scopes')
    expect(text).toContain('profileArn')
  })

  it('标注导入后禁用状态', () => {
    expect(describePreviewItem(previewItem({ authMethod: 'social', disabled: true })))
      .toContain('导入后禁用')
    expect(describePreviewItem(previewItem({ authMethod: 'social', disabled: false })))
      .not.toContain('导入后禁用')
  })

  it('缺少认证类型时明确提示', () => {
    expect(describePreviewItem(previewItem({ authMethod: undefined }))).toContain(
      '类型未识别'
    )
  })

  it('公共客户端不展示 clientSecret', () => {
    const text = describePreviewItem(
      previewItem({
        authMethod: 'external_idp',
        hasClientId: true,
        hasClientSecret: false,
        hasTokenEndpoint: true,
      })
    )
    expect(text).toContain('clientId')
    expect(text).not.toContain('clientSecret')
  })

  it('展示 provider（若有）', () => {
    expect(
      describePreviewItem(previewItem({ authMethod: 'idc', provider: 'BuilderId' }))
    ).toContain('BuilderId')
    // external 的 provider 为空，不应出现占位文本
    const ext = describePreviewItem(
      previewItem({ authMethod: 'external_idp', provider: undefined, hasClientId: true })
    )
    expect(ext).not.toContain('undefined')
  })
})

describe('describeContainer', () => {
  it('四种容器形态都有中文说明', () => {
    expect(describeContainer('FlatArray')).toBe('平铺数组')
    expect(describeContainer('FlatObject')).toBe('平铺单对象')
    expect(describeContainer('Wrapper')).toContain('accounts')
    expect(describeContainer('LegacyNested')).toContain('嵌套')
  })

  it('未知形态原样返回', () => {
    expect(describeContainer('SomethingElse')).toBe('SomethingElse')
  })
})
