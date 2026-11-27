---
status: done
---

# Task Spec：labeled continue 下一轮迭代

task-id: `rule-fp-fn-p17`

## 背景

labeled continue 此前直接并入标签语句后的状态，未让状态进入下一轮循环体：

```ts
outer: for (;;) {
  if (flag) { flag = false; continue outer; }
  flag = true;
  break outer;
}
```

下一轮 `flag=false` 才进入 break 分支；旧逻辑会错误保留 continue 的 false 作为循环后状态。

## 范围

- 匹配 labeled loop 的 continue 状态不再直接作为循环后路径。
- 将 continue 状态合并后回灌循环体，重新分析下一轮。
- 多轮 labeled continue 迭代上限 8 轮；仍有剩余状态时保守并入循环后路径。
- labeled block 等非循环目标的 continue 保持既有保守处理。
- 其他标签继续向外传播。

## 验收

- [x] labeled continue 后下一轮状态参与常量/身份裁剪。
- [x] 下一轮 break 后的状态覆盖 continue 前状态。
- [x] 原有 labeled break/continue 测试保持通过。
- [x] taint 单测与全量门禁通过。

## 排除项

- 循环不动点证明与无限循环可达性判定。
- 跨函数/异常控制流中的标签。
