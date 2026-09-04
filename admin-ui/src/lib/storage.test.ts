import { beforeEach, describe, expect, it } from 'vitest'
import { DEFAULT_PER_PAGE, storage } from './storage'

describe('storage.getPerPage / setPerPage', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('读写往返一致', () => {
    storage.setPerPage(48)
    expect(storage.getPerPage()).toBe(48)
  })

  it('未设置时返回默认值', () => {
    expect(storage.getPerPage()).toBe(DEFAULT_PER_PAGE)
  })

  it('非法值回落默认值', () => {
    localStorage.setItem('credentialsPerPage', 'abc')
    expect(storage.getPerPage()).toBe(DEFAULT_PER_PAGE)
    // 不在可选值内的数字同样回落，避免绕过后端上限
    localStorage.setItem('credentialsPerPage', '9999')
    expect(storage.getPerPage()).toBe(DEFAULT_PER_PAGE)
  })
})
