# Track previews

[English](#track-previews) | [简体中文](#曲目巡听)

Open **Menu -> Preview** for a finite pass through the current music queue, starting at its current
index. The standalone page uses an MD-inspired silver design, with English and Simplified Chinese
labels. This is a track-selection aid, not a new audio-analysis engine.

## Idea and references

I wanted a quick way to rediscover songs in a large library on my NW-A55. I took inspiration from Sony's ZAPPIN: sample short sections of several tracks, then restart the chosen song for full playback.

The [NWZ-B170 guide](https://helpguide.sony.net/gbmig/44142741/v1/eng/contents/03/01/04/04.html) describes approximately 4-/15-second clips and a roughly 45-second start. The [NWZ-W270 guide](https://www.sony-asia.com/microsite/walkman_i-manuals/NWZ-W273/gb/contents/03/01/06/06.html) also describes using prior song analysis to select a recognizable segment, with a fixed-position fallback. These are behavioural references, not a claim that the B170 was the first ZAPPIN model or that all Sony models behave identically.

I also wanted dedicated touch controls and a retro feel, so I chose an MD-inspired silver portable-player page. This implementation adapts that interaction to Cinder's existing queue and Sony playback services, with touch controls, a finite pass, and normal-play bookmark/statistics isolation. It does not port Sony's private ZAPPIN code, algorithm, voice prompts or original UI assets. A new local chorus-analysis tool remains separate future work.

## Controls

- **4 s / 15 s:** choose the preview length. A track with less playable time left uses the remainder.
- **Previous / Next:** move between candidates within the finite pass.
- **Pause / Continue:** pause and resume the preview without changing the normal playback intent.
- **Play this track in full:** leave preview mode, start the selected track from the beginning and
  return to Now Playing.
- **End preview:** leave preview mode. Subsequent normal playback restores the saved queue and
  bookmark; the sample position must not replace the normal bookmark.

The pass stops at its end rather than wrapping indefinitely. An empty queue disables the transport
controls. Preview cannot start in FM, USB-DAC, Bluetooth receiver or USB storage mode. Normal music
over Bluetooth output still uses the existing playback route.

## Where a preview starts

The planner reads existing Sony **SABI** data from the library database. A valid marker takes
priority; otherwise a known-length track starts at `min(45 seconds, duration - preview length)`,
bounded at zero. A track with unknown duration starts at zero. Markers near the end and short tracks
are bounded to the available duration. The page displays the actual source and start position.

No models, Python runtime, new audio decoder, online service or third-party algorithm dependency is
added to the firmware. A Sony marker is an existing candidate, not a guarantee that the segment is
musically the chorus. Songs without valid markers still use the fixed fallback.

## Maintenance map

| Area | Responsibility |
|---|---|
| `player/cinder-ui/src/zappin.rs` | Pure start/length planning and finite-session rules |
| `player/cinder-ui/src/retro_preview.rs` | Page drawing and shared drawing/touch geometry |
| `player/cinder-ui/src/nav.rs` | Navigation, page state and preview actions |
| `player/cinder-ffi/src/lib.rs` | Candidate snapshot, saved normal context, generation checks and handoff confirmation |
| `cinder-home/src/main.cpp` and `src/zappin_policy.h` | Existing Sony playback calls, sample timing, late callbacks and normal-play handoff |
| `player/cinder-ffi/src/stats.rs` and `scrobble.rs` | Keep sample playback out of ordinary listening statistics |
| `cinder-home/harness/scenarios.cpp` | Real main-loop sequencing against fake services, including failure and retry paths |

The pure planner and page are separate modules. Playback integration follows the existing navigator,
FFI runtime and main-loop structure; it introduces no second audio backend or persistent database
schema. It does add state to those shared files, so future playback changes need the regression
checks below. This is a moderate maintenance cost, rather than a display-only feature.

### Contracts to preserve when playback changes

1. A preview owns a bounded candidate snapshot and a generation. A late callback for an older
   candidate or session must not seek, resume or confirm the new one.
2. Normal bookmarks and listening clocks stay protected during previews. A metadata/URI update alone
   does not complete a normal-play handoff; playback status, URI and actual position must agree.
3. Full playback starts at zero. Queue restoration uses its saved bookmark, including an exact zero
   or a bookmark for another URI. Every way out of a preview (End, Full, a refused start, a source change,
   a seek, a handoff not confirmed within 15 s) releases the hold and puts the original track and
   position back, so no path depends on a later confirmation to resume normal statistics.
4. Queue edits, shuffle/repeat changes, explicit selection and source changes cancel or hand over
   ownership intentionally. Normal queue actions outside preview mode keep their existing behaviour.
5. Labels describe state; translated text must never decide transport or source behaviour.

## Checks

The existing player test and host-preview commands include planner, navigation, font/layout and
golden coverage. The harness lists its scenarios automatically, so the `zappin-*` cases are included
in the full harness run:

```sh
python3 tools/generate_locales.py --check
(cd player && cargo test --release)
cinder-home/harness/run.sh zappin-preview zappin-full-same-uri zappin-aba-stale-uri
cinder-home/harness/run.sh zappin-timeout-full zappin-timeout-restore
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full checks and the ARM/device build requirements.
For device regression, exercise both lengths, pause/resume, candidate changes, Full, End followed by
normal playback, finite completion and display sleep/wake. Check both marker and fallback sources;
hearing a fixed-position sample does not validate a chorus marker.

I tested this source on my Sony NW-A55 and found no issues during my trial. I also confirmed that
source toasts no longer cover the controls. A photographed marker-source
preview at 0:50 for Chouchou / 1619khz matched device log seeks of 50,838 ms. This verifies use of an
existing marker; it is not an independent assessment of chorus accuracy or every hardware scenario.
Final ARM link, dependency/GLIBC, loader and ABI checks were performed separately from hosted CI.
The committed `dist/` release payload is not changed by this source PR; a release still requires the
maintainer's normal rebuild and payload-verification process.

Hosted [CI for the feature commit `35b11c3`](https://github.com/superwilso/Cinder/actions/runs/39709040673) passed all 6 jobs, including the full
native harness, player tests, both installer platforms, dependency advisories and payload checks.
The ARM link and device checks above were separate checks, not capabilities of that hosted runner.

In my earlier local VM run, the full native harness was not green: Bluetooth assertion failures and timeouts
also reproduced on unmodified v0.3.19 (`2877c84e`), without track previews, under the same harness/VM.
Paired reruns reproduced the initially candidate-only failing names on the baseline as well. These
observations are not attributed to this preview change. The affected upstream Bluetooth scenarios,
service fakes and VM timeout behaviour should be investigated separately; this does not turn the
full-suite results into a pass. The targeted preview/queue checks passed.

## Footprint

Using the same toolchain, the stripped stable ARM executable grew from 5,690,924 to 5,736,252 bytes:
45,328 bytes (44.27 KiB, approximately 0.80%). The Chinese UI font subset grew by 228 bytes. No model
or additional runtime library is bundled. I have not separately benchmarked RAM or power overhead.

---

# 曲目巡听

[English](#track-previews) | 简体中文

从 **菜单 → 巡听** 打开独立页面，从当前队列的当前曲目开始，按 4 秒或 15 秒试听一轮。这是帮助选歌的功能；页面采用 MD 银色随身听风格，支持中英文。

## 想法来源

我想在 NW-A55 上更方便地从较大的曲库中重新发现想听的歌，因此参考 Sony **ZAPPIN 短片段巡听**：连续听每首歌的一小段，遇到喜欢的歌曲，再从头完整播放。

索尼 [NWZ-B170 官方说明](https://helpguide.sony.net/gbmig/44142741/v1/eng/contents/03/01/04/04.html)介绍了约 4 秒／15 秒片段和约 45 秒起点；[NWZ-W270 官方说明](https://www.sony-asia.com/microsite/walkman_i-manuals/NWZ-W273/gb/contents/03/01/06/06.html)还说明可利用事先完成的歌曲分析来选取有代表性的片段，缺少结果时回退到约 45 秒。这些是交互思路的参考，不表示 B170 是历史上首款 ZAPPIN 设备，也不表示不同索尼机型的实现完全相同。

我也希望巡听拥有独立的触屏控件和复古感，因此采用 **MD 银色随身听**风格的页面。因此本版结合了短片段选歌和独立触屏操作页，围绕 Cinder 已有队列与索尼播放服务重新实现；没有移植 Sony 私有 ZAPPIN 代码、算法或提示音，也没有复制索尼原机界面资源。有限的一轮巡听、断点和统计隔离、独立页面及触屏控件，是本次对 Cinder 使用场景的设计。

这版只读取设备数据库中已有的 SABI 起点；我没有为它新增副歌检测器。“电脑离线分析后将结果交给播放器”仍是后续独立设想，不属于本 PR。

## 如何使用

- **4 秒／15 秒：** 选择试听长度；接近结尾或短曲目会缩短到剩余可播放范围。
- **上一首／下一首：** 在本轮候选范围内切换曲目。
- **暂停／继续：** 暂停或继续当前片段，不将它当作普通完整播放。
- **就听这首 · 完整播放：** 结束巡听，从所选歌曲开头播放，并回到普通播放页面。
- **结束巡听：** 离开巡听。之后恢复普通播放时，使用保存的原队列与断点，不能让试听位置覆盖原断点。

本轮到末尾后停止，不无限循环。空队列禁用播放控件。FM、USB-DAC、蓝牙接收和 USB 存储模式下不能启动巡听；普通音乐的蓝牙输出继续使用现有播放通路。

## 起点如何选择

优先读取 Sony 数据库中已有的有效 **SABI** 标记。没有有效标记、但知道曲长时，起点为 `max(0, min(45000, 曲长毫秒 - 试听毫秒))`；不知道曲长时从头尝试播放。临近结尾的标记和试听长度都会限制在可播放范围内。页面显示实际采用的来源和起点。

固件没有增加模型、Python 环境、解码器、在线服务或第三方识别算法依赖。已有标记只是候选位置，不保证音乐语义上一定是副歌。

## 代码位置与维护

| 文件或模块 | 职责 |
|---|---|
| `player/cinder-ui/src/zappin.rs` | 起点／时长规划、有限会话的纯逻辑 |
| `player/cinder-ui/src/retro_preview.rs` | 独立页面绘制与共用的绘制／点击几何 |
| `player/cinder-ui/src/nav.rs` | 页面状态、导航及操作意图 |
| `player/cinder-ffi/src/lib.rs` | 候选快照、普通播放上下文、会话代次与交接确认 |
| `cinder-home/src/main.cpp`、`zappin_policy.h` | 调用原播放服务、片段计时、迟到回调与普通播放交接 |
| `player/cinder-ffi/src/stats.rs`、`scrobble.rs` | 隔离试听与正常听歌统计 |
| `cinder-home/harness/scenarios.cpp` | 对真实主循环的操作顺序、失败与重试测试 |

维护成本是**中等**。规划器和页面已独立，但播放交接仍接在原导航、FFI 与主循环中，不是只增加一个画面。后续修改队列、随机／重复播放、切源、统计或断点时，需要同时运行相关回归检查。没有新建第二套音频后端或持久化数据库结构；纯界面改动通常只需关注页面与布局。

必须保留以下约束：

1. 每轮和每个候选有明确身份；旧回调不能对新候选定位、继续播放或完成确认。
2. 巡听期间保护原断点和正常播放时钟。仅更新标题或 URI，不能解除普通播放交接保护；状态、URI 和实际位置要符合预期。
3. 完整播放从零开始；恢复原队列使用保存断点，包括精确零点和不同 URI。结束、完整播放、拒绝启动、切换来源、拖动进度或 15 秒内未确认的交接，都会释放保护并恢复原曲目与位置。
4. 修改队列、随机／重复状态、明确选曲或切源时，要有明确的取消／交接；普通播放状态下的队列操作不能被误当作巡听。
5. 显示文字只描述状态，不能根据中英文标签判断开关或播放行为。

## 代码量与固件体积

同工具链下，主程序从 5,690,924 字节变成 5,736,252 字节，增加 **45,328 字节，即 44.27 KiB、约 0.80%**。这里比较的是 ARM 主程序，不是整机系统镜像。没有捆绑模型或新增运行库，不能仅凭文件体积推断 RAM 或耗电；这两项尚未专门测量。

## 验证与边界

已完成的本地记录包括：42 项相关主程序场景；ARM FFI 179 项（仓库测试 178 项及独立探针 1 项）；UI 652、字体 2、布局 13、host 4、数据库 36 项；格式、翻译生成、规定 Clippy 组和差异检查。最终 ARM 主程序完成链接、依赖／GLIBC、隔离加载及 Sony ABI 预检。这些为本地检查记录。正式 [GitHub CI（功能提交 `35b11c3`）](https://github.com/superwilso/Cinder/actions/runs/39709040673) 的 6 项作业也已全部通过，包括完整 native 场景、播放器测试、Linux／Windows 安装器、依赖审计和载荷检查；ARM 链接与真机验证另行完成。

**我已在自己的 Sony NW-A55 上实机试用，测试中没有发现问题。** 我也确认重复来源提示已消失、起点显示正常。《1619khz》页面显示“副歌起点 0:50”，设备日志定位为 50,838 毫秒，确认已有 SABI 被读取和使用；没有直接导出数据库做 SQL 核对，也没有独立听评副歌标记的语义准确性，不声称设备清单每项都单独验证过。

此前本地虚拟环境检查没有全绿：蓝牙场景断言失败和超时，已在未加入巡听的原版 v0.3.19（`2877c84e`）同环境复现；配对复验也复现了首次候选独有的失败场景名称。这些现象不是本 PR 独有的问题，不应归因于本次巡听改动。**建议单独核查上游相关蓝牙场景、服务模拟及虚拟环境超时原因**；保留原始失败结果，不将全量检查记作通过，也不直接认定是真机故障。

此前声景性能／响度检查及 installer、sim／workspace 的本地验证有环境限制；正式 CI 的相关作业随后通过，不能将旧本地环境限制当作当前 CI 失败。源码 PR 没有替换仓库内 `dist/` 发布载荷；正式发布仍需要维护者按原流程重建并核验。

## 维护检查入口

```sh
python3 tools/generate_locales.py --check
(cd player && cargo test --release)
cinder-home/harness/run.sh zappin-preview zappin-full-same-uri zappin-aba-stale-uri
cinder-home/harness/run.sh zappin-timeout-full zappin-timeout-restore
```

完整环境要求见 [贡献说明](../CONTRIBUTING.md)。真机回归应覆盖两种时长、暂停／继续、切换候选、完整播放、退出后正常播放、结束一轮和灭屏唤醒，并区分 SABI 与固定回退来源。
