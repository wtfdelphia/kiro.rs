const API_KEY_STORAGE_KEY = 'adminApiKey'
const PER_PAGE_STORAGE_KEY = 'credentialsPerPage'

/** 每页条数的合法取值，与后端 clamp 上限（100）一致 */
export const PER_PAGE_OPTIONS = [12, 24, 48, 100] as const
export const DEFAULT_PER_PAGE = 12

export const storage = {
  getApiKey: () => localStorage.getItem(API_KEY_STORAGE_KEY),
  setApiKey: (key: string) => localStorage.setItem(API_KEY_STORAGE_KEY, key),
  removeApiKey: () => localStorage.removeItem(API_KEY_STORAGE_KEY),

  /** 读每页条数；缺失或不在可选值内时回落默认值 */
  getPerPage: (): number => {
    const raw = Number(localStorage.getItem(PER_PAGE_STORAGE_KEY))
    return (PER_PAGE_OPTIONS as readonly number[]).includes(raw) ? raw : DEFAULT_PER_PAGE
  },
  setPerPage: (perPage: number) =>
    localStorage.setItem(PER_PAGE_STORAGE_KEY, String(perPage)),
}
