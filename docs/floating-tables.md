# 浮动表格（tblpPr）布局与分页规则说明（初稿）

> 状态：初稿。目的是把散落在各处的分支写成一套可审查的规则，
> 作为后续把逻辑收敛成状态机、在现有 fixture 上做零退步重构的依据。
> 所有「Word 实测」均指 Word for Mac 16.114 的 PDF 导出。

## 1. 代码分布

| 关注点 | 位置 |
| --- | --- |
| 浮动表格入口、定位、执行 | `crates/docx-layout/src/hooks.rs:970` `layout_floating_table`（`hooks.rs:1122` `match plan` 分派） |
| 分支选择（纯函数，B0–B5a） | `crates/docx-layout/src/float_plan.rs:139` `plan(&FloatInputs) -> FloatPlan`，单测同文件 |
| 兼容模式 | `crates/docx-layout/src/types.rs` `WordLayoutEra::{Legacy, Word2013}`，`TableBlock::layout_era()` |
| 行循环（含首片截止线、续页整行搬移） | `hooks.rs:328` `layout_table_with_position_and_limit(.., Limit, ContinuationRule)` |
| 行边界带（boundary band） | `hooks.rs:142` `row_cut_band`、`hooks.rs:154` `trailing_band`；绘制在 `display_list.rs` |
| 锚点前文字绕排（legacy） | `crates/docx-edit/src/engine.rs` `pre_anchor_wrap`；轮数上限 `MAX_WRAP_STABILIZE_ROUNDS` |
| 拆分浮动表格的重排与后继段落换页 | `engine.rs` 的不动点迭代器 `stabilize_flow_wrapping`（状态 `WrapStabilizeState`，收敛判定 `converged`） |
| 绕排区域的测量 | `crates/docx-layout/src/measure_blocks.rs:1001` 起的 `measure_blocks_with_*`、`resets_float_flow` |
| 逐行避让浮动对象 | `crates/ooxml-text/src/measure/line_filler.rs:192` `skip_obstructing_floats`，`floats.rs:17` `MIN_WRAP_SEGMENT_WIDTH = 24`（`ooxml_text::measure` 导出；docx-layout 的 `floating_objects::MIN_WRAP_SEGMENT_WIDTH` 引用它，满宽判断用的也是它） |
| 页眉页脚里的浮动对象 | `engine.rs` `header_footer_float_bands` |

同一组语义分布在 4 个 crate 的 6 个文件里，而且布局（hooks）和引擎（engine）之间靠不动点循环来回传递信息。

## 2. 输入维度

一张浮动表格的行为由下面这些量决定（括号里是代码中的名字）：

| 维度 | 取值 | 备注 |
| --- | --- | --- |
| 垂直锚（`vert_anchor`） | `page` / `margin` / `text`（缺省 `text`） | |
| 水平锚（`horz_anchor`） | `page` / `margin` / `text`（缺省 `margin`） | |
| 垂直位置 | `tblpY` 数值 / `tblpYSpec`（`top`、`center`、`bottom`、`outside`、`inline`）/ 都没有 | 没有显式 Y 时先 `ensure_fits` |
| 水平位置 | `tblpX` / `tblpXSpec`（`left`、`right`、`center`、`inside`、`outside`）/ `jc` | `inside`、`outside` 按页码奇偶翻转 |
| 兼容模式（`compatibility_mode`） | `< 15`（缺省按 12）= **legacy**；`>= 15` = **word2013** | 分页行为的主开关 |
| 满宽（`full_width`） | 表格两侧可绕排宽度都 `< 24px` | 文字无法绕排，只能上下避让 |
| 超过一栏（`overflows_column`） | 表高 > 栏高 | |
| 原地分片（`fragments_in_place`） | `tblpY >= 0`、不是 `inside`/`outside`、单栏 | |
| 单栏 | `content_width == column_width` | **多栏下大部分规则直接不生效** |
| 位置与笔尖 | `y` 和 `pen_y`、`content_top`、`content_limit` 的关系 | 决定「在已有文字上方」等情况 |

## 3. 入口分支（`float_plan::plan`，按代码顺序）

`layout_floating_table` 先计算 x、y（第 2 节的锚与位置），把结果和页面状态装进 `FloatInputs`，
由纯函数 `plan` 按顺序判断，**第一个命中的分支生效**，返回 `FloatPlan`；执行留在 `layout_floating_table` 的 `match`。
B0 要在定位（可能 `ensure_fits` 换页）之前判断，所以入口先调用同一个 `falls_back_inline`，`plan` 里再判断一次。

| 分支 | `FloatPlan` |
| --- | --- |
| B0、B4 | `Inline` |
| B1、B2 | `MoveToNextPage` |
| B3 | `Split { first_limit, continuation, zero_space_before, full_width }`；legacy page 锚为 `LegacyDeadline(..)` + `MoveRowsWhole`，其余为 `TextBottom` + `SplitMidRow` |
| B5 | `Whole { full_width: None }` |
| B5a | `Whole { full_width: Some(ClearOrPush \| Push) }` |


| # | 条件 | 行为 | 依据 / fixture |
| --- | --- | --- | --- |
| B0 | `overflows_column && !fragments_in_place` | 当作行内表格排（`layout_table`） | — |
| B1 | 满宽 + `page` 锚 + 显式 `tblpY` + 单栏 + 完全落在正文区 + 与本页一张**完整、未拆分的行内表格**重叠且挤不下（`float_plan.rs:161`） | 强制换页，在下一页重试 | 只有 `place.rs` 单测，无 Word fixture |
| B2 | **word2013** + `page` 锚 + 显式 `tblpY` + 单栏 + `y < pen_y`（位置在已有文字上方）+ 本页已有文字 + 放不下（`float_plan.rs:180`） | 整表移到下一页，在相同 Y 开始拆分 | `table-split-bands/continuation-compat15*.docx` |
| B3 | 单栏 + （`text` 锚 或 超过一栏）+ 不是 `inside`/`outside` + （`y >= pen_y` 或 legacy 的 page 浮动在正文区内）+ 放不下（`float_plan.rs:198`） | **拆分**：笔尖移到 y，进入行循环；legacy 的 page 锚带「首片截止线」（见 3.1） | `floating-table-split/*`、`floating-table-zone/*`、`table-split-bands/*` |
| B4 | `overflows_column`（前面都没命中） | 当作行内表格排 | — |
| B5 | 其余情况 | 整表作为一个浮动片段放在 (x, y) | `floating-table-wrap/*` |
| B5a | B5 + 满宽 | page 锚且在正文区内时，尝试 `clear_float_band` 重排周围文字；否则笔尖推过表底加 `bottomFromText` | — |

### 3.1 legacy 首片截止线（`float_plan.rs:233`，即 `Limit::LegacyDeadline`）

```
float_page_limit = min(footer_line, max(y, content_top) + body_height - text_used)
footer_line      = page_height - footer_distance   // footer 缺省 47.25px
text_used        = pen_y - content_top             // 本页已排的正文
```

含义：浮动表格和它之前的正文共用一页的正文高度预算，但可以伸进下边距，最多到页脚线。
上界来自 LibreOffice 的 `GetFlyAnchorBottom`（「Word <= 2010」分支）。
word2013 没有这条截止线，按文字区底部拆分，和行内表格一样。

### 3.2 拆分结束后笔尖放哪里（`hooks.rs:1153`）

- 满宽：最后一片的底部 + `bottomFromText`
- 没跨页：回到原来的 `pen_y`（后续文字从表格旁边开始绕排）
- 跨了页：回到最后一片的顶部（文字在最后一片旁边继续绕排），并清掉段前间距

另外：非满宽、非 text 锚溢出时，拆分期间段前间距被置 0（`FloatPlan::Split::zero_space_before`，执行在 `hooks.rs:1140`），结束后恢复。

## 4. 行循环规则（`layout_table_with_position_and_limit`）

行循环只读入参 `Limit`（R1）和 `ContinuationRule`（R6），不再自行推断浮动类型。

| # | 规则 | 位置 | fixture |
| --- | --- | --- | --- |
| R1 | `Limit::LegacyDeadline` 时，首片开始把本页的 `content_limit` 临时改成截止线，退出时恢复 | `hooks.rs:361` | `table-split-bands/continuation-compat14`、`rows-compat14` |
| R2 | `cantSplit` 或固定行高的行：剩余空间放不下而下一页放得下，就整行移到下一页 | `hooks.rs:401` | `place.rs` 单测 |
| R3 | 只有首片吃掉延迟的段前间距 | | |
| R4 | 有表头行时，首片要给第一行正文留位置，否则表头和第一行一起走 | | |
| R5 | 一行整体放得下（含下方边界带 `trailing_band`）就整行放下 | `hooks.rs:606` | `table-split-bands/*` |
| R6 | **legacy page 浮动的续页片段整行搬移**：放不下的行移到下一页，不在行中间切；除非它在空白新页上也放不下。首片仍然可以在截止线处从行中间切。由 `ContinuationRule::MoveRowsWhole` 开启 | `hooks.rs:627` | `floattable-avoid-last-manip-ofst`（注释里提到，**仓库里没有**）、`table-split-bands/rows-6-before`（`table_split_pre_anchor_wrap.rs`） |
| R7 | 行中间切：在不超过预算的最深一整行处断开；预算 = min(剩余高度, 文字剩余 - 续行边界带) - 已用 - 本行切口边界带 | `hooks.rs:663` | `table-split-bands/continuation-*` |
| R8 | 行中间切的位置在边界带下方重画边界带，文字行不能越过边界带 | `display_list.rs` | `table-split-bands/continuation-clipping`、`page-bottom-boundary`（只检查「不交叉」） |

## 5. 跨模块的耦合

「浮动排斥区 ↔ 文字」的相互依赖由 engine 的**一个**不动点迭代器收敛
（`engine.rs` 的 `stabilize_flow_wrapping`，状态结构 `WrapStabilizeState`）：

- 每轮用当前测量跑一遍排版，从结果导出下一轮的全部输入：锚定形状的绕排偏移、
  各段栏宽与栏带、表格测量帧（`TableMeasurementFrame`：绕排区、拆分片旁的
  `last_float_fragment_height`、锚点前绕排区 `pre_anchor_wrap`）、`reflow_breaks` 集合。
- **收敛判定**是纯函数 `WrapStabilizeState::converged`：连续两轮导出相同状态，
  即两轮排版结果相同，下一轮必然复现——不动点。
- 超过轮数上限（`MAX_WRAP_STABILIZE_ROUNDS`，不均衡栏/连续分节随文档长度增长，
  其余模式随浮动对象数增长）时**不静默截断**：向 stderr 报告锚点与轮数
  （`report_wrap_stabilize_giveup`），保留最后一轮结果。渲染永远不因不收敛而失败。

耦合的三个来源都在这个迭代器里：

1. **锚点前文字绕排（legacy）**：Word <= 2010 把 page 浮动所在的整页围绕它重排，
   所以锚点**之前**的段落也要绕排。做法是：先排一遍拿到首片的位置，回填一个
   `PreAnchorWrap` 区域给测量，再排一遍。该区域表达的是「浮动所在那一页」的排斥区，
   测量却按流坐标理解：不设上限的话，区域会越过锚点笔尖，把后继段落（以及更后的流）
   当作仍在浮动旁边一样压窄。因此区域止于**锚点前那段文字的流范围**（其测量高度，
   含段间距）——后继段落按拆分片旁的区域绕排。fixture：`table-split-bands/continuation-6-before`、
   `continuation-25-before`、`rows-6-before`；matrix 的 `ycenter-c14`、`ybottom-c14`。
2. **拆分后的后继段落换页**：拆分表格结束的那一页之后，后继段落要按满宽重新测量；
   为此迭代器的状态里有 `reflow_breaks` 集合。
3. **逐行避让**：`line_filler` 用「默认字号的探针」决定跳过浮动对象，用「行盒」决定绕排边距，两者曾互相打架（2d359b30）。
   Word 的规则是「行盒碰到浮动对象顶部就算绕排」，并且要计入此前所有段落的段间距（c08468b0，`image-wrap-flow`）。

## 6. fixture 覆盖矩阵

| fixture 组 | 数量 | 覆盖的规则 | 校验方式 |
| --- | --- | --- | --- |
| `floating-table-split` | 30 个 docx + `word-page-matrix.json` | B3、R5–R7、后续文字的绕排与换页、widow / keep 控制 | 逐行比对 Word 坐标 |
| `floating-table-zone` | 6 | B3、最后一片旁边的绕排、片底含底边框 | 逐行比对 Word 坐标 |
| `floating-table-wrap` | 7 | B5（不拆分时左右绕排宽度） | 行数和指标比对 Word |
| `table-split-bands` | 9 | B2、R1、R6–R8、legacy 与 word2013 的差异 | 关键基线坐标 + 不交叉检查 |
| `image-wrap-flow` | 1 | 浮动图片与段间距的累计 | 首行坐标 |
| `header-float-wrap` | 8 | 页眉页脚浮动对象对正文的避让 | 正文行框 |
| `floating-table-matrix` | 16（8 种组合 × c14/c15） | 下面 6.1 列出的原空缺 | **逐词**比对 Word，按用例记基线，只许变好（棘轮） |

> `floating-table-split` 的 fixture 都是 `compatibilityMode = 15`。

### 6.1 补齐的组合与 Word 实测行为（`floating-table-matrix`）

这些组合原来没有 fixture。下表是 Word 的实际行为和引擎目前的差距；
数字为「不一致的词 / Word 页面上的词」，`floating_table_matrix.rs` 的 `BASELINE` 里记的就是这些数。

| 组合 | Word 的行为 | 引擎现状 | c14 | c15 |
| --- | --- | --- | --- | --- |
| `margin` 锚拆分 | c14 原地拆分，后续文字在最后一片旁绕排；**c15 整表移到下一页**（和 B2 一样，但 B2 只认 page 锚 + 显式 tblpY） | c14 表格对，后续文字没绕排；c15 没换页 | 160/284 | 280/284 |
| `margin` 锚 + 锚点前有长段落 | 原地拆分；c14 首片越过文字底部（r143 在 727.92pt） | c15 一致；c14 没用 legacy 截止线（3.1 节只认 page 锚），单元格 x 偏 5.8pt（legacy 左移没生效） | 280/340 | 0/340 |
| 两栏 | **不拆分**：表格整体上移到页底对齐，顶部 33 个词被裁到页外 | 当作正常拆分处理 | 254/221 | 254/221 |
| `tblpXSpec=outside` | 第 1、2 页都靠右：续片沿用首页的一侧，不按页码奇偶翻转 | 被排除，退化成行内表格（B0），x 和 y 都错 | 340/344 | 340/344 |
| `tblpYSpec=bottom` | 从 174.72pt 开始（底对齐减去溢出部分）；c14 越过页脚排到 789pt，c15 在正文底部拆分 | 放在笔尖处 | 250/284 | 280/284 |
| `tblpYSpec=center` | c14 留在第 1 页 93pt，**锚点前的段落也绕排**；c15 移到下一页 | 放在笔尖处 | 254/284 | 280/284 |
| 满宽浮动压在行内表格上（B1） | c14 浮动表格拆分（第 1 页 6 行、第 2 页 2 行）；**c15 移到下一页**——B1 得到 Word 实测确认，但只在 c15 成立 | 两种模式都没换页；c14 下行内表格与浮动表格同高的行，Word 整行推到浮动表格右侧（x≈315），引擎不处理「行内表格绕排浮动表格」 | 104/108 | 68/108 |
| `text` 锚 + 锚点前有文字 | 原地拆分 | c15 一致；c14 只差单元格 x 偏 5.8pt | 273/310 | 0/310 |

由此修订第 3 节的规则：

- **B2 的范围太窄**：Word 2013+「浮动表格放不下就整表移到下一页」不只适用于 page 锚 + 显式 tblpY，`margin` 锚和 `tblpYSpec=center` 也一样。真正的条件更可能是「位置固定的浮动表格（非 text 锚）在本页放不下」，需要再补一个「margin 锚、位置在已有文字下方」的用例来确认。
- **B1 只在 c15 成立**：c14 下 Word 原地拆分。代码里 B1 没有判断兼容模式。
- **多栏**是一条独立规则：不拆分，整表上移贴住页底，超出部分被裁掉。
- **inside / outside**：续片沿用首页的一侧。
- **tblpYSpec + 拆分**：先按整表高度对齐，再按溢出量回推起点。
- **legacy 表格 x 位移**（Word 实测约 100 份，实验在 `~/docx-eval/work/diag-legacy-shift/`）：compat <= 14 时，左对齐的浮动表格**无条件**左移 T（单元格左边距 + 左边框内缩），行内表格在有表格样式或 tblInd 时左移 T，右对齐（含 `outside` 落在右侧）**右移** T，居中不移；c14 下浮动表格的 tblInd 生效、c15 忽略；绕排区跟着表格一起移。引擎的 `table_compat_leading_shift` 只在行内路径生效、只移单元格边距，浮动路径完全不移；`margin-c14` 等碰巧正确，是因为它们落进 B4 走了行内路径。
- **行内表格不绕排浮动表格**：c14 下 Word 把与浮动表格同高的行内表格行整行推到浮动表格右侧，引擎只对段落绕排。
- **legacy 截止线（3.1）不只属于 page 锚**：`margin-pretext-c14` 里 Word 的 margin 锚首片也越过文字底部（r143 在 727.92pt > 720pt）。
- **margin 锚、位置高于笔尖（legacy）**：`page_float_above_pen` 只认 page 锚，于是 `margin-c14` 落进 B4 按行内表格排，后续文字不绕排最后一片；Word 从笔尖起排、照常拆分和绕排。
- （更正：初稿里的「首片少一行」是测试提取的错误——跨页切开的行在两页各画一份再裁剪，提取时没按裁剪框过滤；已修，c15 的两个用例完全一致。）

**仍未覆盖**：

- R6 引用的 `floattable-avoid-last-manip-ofst`、`floattable-footer-2rows` （LibreOffice 的测试文档）不在仓库里，只出现在代码注释和 `floating-table-zone/word.json` 的备注中；`measure_blocks.rs` 还引用了同样不在仓库里的 `floattable-negative-height.docx`

## 7. 可疑之处

1. **兼容模式缺省**：`compatibility_mode` 为 `None` 时，B2 判为非 word2013、B3 判为 legacy。这和 Word 一致（缺省 = 12），
   但 `engine.rs:619` 用的是 `is_some_and(>= 15)` 取反，语义一样、写法不同，容易改错。
2. **魔数**：页脚距离缺省 `47.25`、满宽阈值 `24`、`pen_y > content_top + 0.5`、`f64::EPSILON` 比较栏宽。
   其中 24 和 `MIN_WRAP_SEGMENT_WIDTH` 是同一个概念，却在两个 crate 里各写一份。
3. **由 fixture 反推的规则**：B1 的条件长达 10 项，只为一个场景；R6 的「除非在空白新页上也放不下」写在注释里，依据是仓库里不存在的 fixture。
4. **4 轮上限（已消除）**：锚点前绕排曾依赖「最多 4 轮」的静默截断；现在是统一迭代器
   的不动点收敛（§5），超上限会报告而不是静默采用。
5. **边界带反复改动**：08f696e4、acf42ef0、6258df9e、b31f3147 先后 4 次修改同一个概念，
   说明「行带属于哪一行、切口时谁来画」没有定义清楚。

## 8. 重构方向（待讨论）

把第 3、4 节合并成一个显式的决策函数和一个分片状态机：

```rust
enum FloatPlan {
    Inline,                                   // B0、B4
    MoveToNextPage,                           // B1、B2
    Split { first_limit: Limit, continuation: ContinuationRule }, // B3
    Whole { band: BandPolicy },               // B5、B5a
}

enum Limit { TextBottom, LegacyDeadline(f64) }               // 3.1
enum ContinuationRule { SplitMidRow, MoveRowsWhole }         // R6：word2013 / legacy
```

- `plan(&FloatInputs) -> FloatPlan` 是纯函数，输入就是第 2 节的维度，可以单独穷举测试；
- 行循环只读 `Limit` 和 `ContinuationRule`，不再自己判断 `float_page_limit.is_some() && floating_x.is_some()`；
- 兼容模式统一封装成 `WordLayoutEra::{Legacy, Word2013}`，消灭散落的 `>= 15` / `< 15`；
- 边界带定义成表格测量的一部分（每行上带、下带，以及切口时谁负责重画），绘制和分页共用同一份定义；
- 门禁：第 6 节所有 fixture 零退步，外加 docx-eval 语料 `deval diff` 零退步。
