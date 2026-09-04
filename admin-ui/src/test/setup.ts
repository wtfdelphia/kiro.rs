import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

// Node 26 自带一个 localStorage getter，未开 --experimental-webstorage 时返回 undefined，
// 且它覆盖了 jsdom 挂上的实现。把 jsdom 内部的那份接回来，否则读写 localStorage 全部报错。
const jsdomLocalStorage = (window as unknown as { _localStorage?: Storage })._localStorage
if (jsdomLocalStorage && !globalThis.localStorage) {
  Object.defineProperty(globalThis, 'localStorage', {
    value: jsdomLocalStorage,
    configurable: true,
    writable: true,
  })
}

afterEach(() => {
  cleanup()
})
