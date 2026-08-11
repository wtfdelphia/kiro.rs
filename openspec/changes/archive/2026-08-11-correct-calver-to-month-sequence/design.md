## Context

`scripts/check_release_version.py` 现有判定：

```python
CALVER_TAG = re.compile(r"^v(\d{4})\.([1-9]\d?)\.([1-9]\d?)$")
...
year, month, day = (int(value) for value in match.groups())
try:
    dt.date(year, month, day)          # 第三段被当作日历日
except ValueError as error:
    raise ReleaseVersionError(f"release tag {tag!r} is not a valid calendar date")
```

正则本身只要求「1-2 位、无前导零」，真正把第三段绑定为日期的是 `dt.date()` 调用。因此格式修正的
核心是替换该语义校验，正则仅需微调（月份上界）。

规格侧对应句（`openspec/specs/release-version-governance/spec.md:13`）：

> 正式 tag MUST 严格使用 `vYYYY.M.D`，其中日期 MUST 是有效日历日期 …… 正式版本 MUST NOT 使用
> 修订后缀，因此同一自然日 MUST NOT 创建第二个正式版本。

## Goals / Non-Goals

**Goals:**

- 把第三段语义由日历日修正为当月发布序号，恢复同月多版本发布能力。
- 保留全部与版本身份无关的既有约束与门禁接线，使本次修正的爆炸半径限于「格式判定」一处。
- 让规格文本自身记录这次误判与纠正，避免后续再次按日期解读历史 tag。

**Non-Goals:**

- 不改写任何历史 tag 或已发布 Release。
- 不改 `version-gate.yaml` 的接线、权限或 caller 的 `needs` 依赖。
- 不改 Docker、Release 资产命名、OCI label 或任何 Rust 源码。
- 不引入自动序号推导（不让门禁替维护者计算下一个序号）。
- 不转向 SemVer，不引入第四段。

## Decisions

### D1. 第三段为当月发布序号，不是日历日

格式 `vYYYY.MM.MICRO`：`YYYY` 四位年，`MM` 月份 1-12 不补零，`MICRO` 当月发布序号，从 1 开始、
不补零、无上界。

校验相应替换：

```python
CALVER_TAG = re.compile(r"^v(\d{4})\.([1-9]\d?)\.([1-9]\d*)$")
...
if not 1 <= month <= 12:
    raise ReleaseVersionError(f"release tag {tag!r} has invalid month {month}")
```

两处正则调整值得说明：

- 月份段保持 `[1-9]\d?` 并新增范围校验。原实现依赖 `dt.date()` 顺带拒绝 13-99 月，移除日期校验后
  必须显式补上，否则 `v2026.99.1` 会通过。
- 序号段由 `[1-9]\d?` 放宽为 `[1-9]\d*`。原上界 99 来自「日期最多两位」，序号无此理由；仍禁前导零
  与 0 值。

采用 CalVer 官方术语记录本方案：year=major slot、month=minor slot、micro=patch slot，与 Twisted
一致。

### D2. 同月多版本靠序号单调递增保证唯一

移除「同一自然日只允许一个正式版」。原约束的实际作用是保证版本唯一且可排序——日期天然唯一，故当时
无需额外规则。改为序号后，唯一性必须显式表述：同一年月内序号 MUST 严格递增且 MUST NOT 复用。

不让门禁自动校验「序号是否恰好等于上一个 +1」。理由：门禁只看单个 tag 的自身合法性与 main 可达性，
引入「与历史 tag 比较」会让判定依赖远端 tag 列表的完整性（浅克隆、tag 未同步都会误判），且跳号本身
无害。唯一性由 git 保证——同名 tag 无法重复创建。

### D3. 不追溯修正历史 tag

`v2026.7.27` 起若干 tag 第三段与日期重合。在序号语义下它们仍然合法（7 月第 27、28、30、31 次发布
虽与实际发布次数不符，但单调递增、无冲突），且改写已发布 tag 会破坏用户已拉取的镜像与二进制引用。

代价是历史区段的序号存在语义空洞（7 月并未真的发布 31 次）。接受该痕迹，在文档中说明其来源，优先
保证已发布产物引用稳定。

### D4. 保留其余全部身份约束

不变部分：附注 tag（`ls-remote` peeled 行判定）、Cargo 版本一致、tag 指向发布提交、
`origin/main` 可达、无修订后缀、月份与序号不补零、人工发布只能从当前提交唯一附注 `v*` tag 推导。

`version-gate.yaml` 与两条 caller workflow 零改动：本次只换 `validate_release` 内部判定，命令行
接口与 job 接线不变。上一轮的红/绿路径接线证据因此仍然有效，但判定规则已变，需重新取证。

## Risks / Trade-offs

- [移除 `dt.date()` 后月份失去隐式校验] → D1 显式补 1-12 范围检查，并加 `v2026.13.1` 反例测试。
- [序号段放宽上界后正则更宽松] → 仍禁 0 与前导零；配合 Cargo 一致性，畸形序号无法单独通过门禁。
- [历史序号语义空洞] → D3 接受并文档化，换取已发布产物引用稳定。
- [维护者误按日期填序号] → 文档明确「序号≠日期」，并在 README 发版清单给出「查上一个同月 tag 后
  加一」的操作步骤。
- [规格连续两次修改同一 Requirement 造成理解负担] → delta 用 MODIFIED 并在规格正文保留一句纠正
  说明，使读者知道为何格式变过一次。

## Migration Plan

1. 修改门禁正则与校验逻辑，替换错误文案。
2. 替换 `test_rejects_invalid_calendar_date`（语义已失效），补同月多版本正例、非法月份、序号 0 与
   前导零反例。
3. 更新长期规格 delta、README 与设计文档修订记录。
4. 本地跑准绳与门禁测试；`openspec validate --all`。
5. 重跑 CI 红路径（Cargo 失配的临时 tag）与绿路径（`v2026.8.11` 正式发布，含 Opus 5）。

回滚：门禁改动为单文件局部修改，可直接回退；未改写任何历史 tag 或已发布产物，回退无外部副作用。

## Open Questions

无。下一个正式版号为 `v2026.8.11`（8 月第 11 次发布），已核实远端不存在。
