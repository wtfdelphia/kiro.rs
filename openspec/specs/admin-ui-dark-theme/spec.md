# Capability: admin-ui-dark-theme

## Purpose

Ensure Admin UI dropdown/select controls (model selection, endpoint selection, credential auth method) remain readable in dark mode by providing a reusable themed select component built on design tokens instead of unthemed native option lists, without regressing existing dark-mode primitives.

## Requirements

### Requirement: Admin 下拉控件在黑夜模式下可读

Admin UI dropdown/select controls used for model selection, endpoint selection, and credential auth method selection MUST remain readable in dark mode when expanded. Controls MUST NOT rely on unthemed native option lists that render as low-contrast white panels in dark theme.

#### Scenario: 测试模型下拉 dark 展开

- **WHEN** 操作者启用 dark 模式并打开凭据测试对话框展开模型列表
- **THEN** 下拉面板背景与选项文字对比清晰可读，不得出现整片白底导致选项不可辨

#### Scenario: 设置端点下拉 dark 展开

- **WHEN** 操作者在 dark 模式下打开运行时设置并展开默认端点列表
- **THEN** 选项列表使用主题背景/前景 token（或等价主题化组件）保持可读

#### Scenario: 添加凭据认证方式下拉 dark 展开

- **WHEN** 操作者在 dark 模式下打开添加凭据并展开认证方式
- **THEN** 选项列表可读且与页面 dark 主题一致

### Requirement: 主题化选择组件复用

Admin UI MUST provide a reusable themed select/listbox component (or equivalent) for operator-facing enumerations, using design tokens such as background/popover/foreground already defined for light and dark themes.

#### Scenario: 组件使用主题 token

- **WHEN** 主题化 Select 在 dark 类作用于 documentElement 时渲染
- **THEN** trigger 与 content 使用 CSS 变量主题色，而不是硬编码纯白背景

### Requirement: Dark 模式控件抽检不回归

Primary Admin actions implemented with existing Button/Input/Dialog primitives MUST continue to be usable in dark mode after select theming work. Hardcoded low-contrast utility colors in critical paths SHOULD be avoided or paired with dark variants.

#### Scenario: 顶栏与卡片主按钮 dark 可用

- **WHEN** dark 模式下查看 Dashboard 顶栏与凭据卡片主操作按钮
- **THEN** 按钮标签与文字可读，点击目标可辨
