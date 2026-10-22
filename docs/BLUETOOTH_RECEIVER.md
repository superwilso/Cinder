# Bluetooth receiver codecs

## English

Cinder can ask Sony's `BtPlayerService` which receiver codecs it may negotiate with a phone. The receiver preference is independent of the Bluetooth transmitter setting and is stored as `rx_codec` in the existing settings file. Older files, malformed values, and values outside 0–3 use Auto.

The choices are Auto (offer AAC and LDAC), LDAC priority (offer LDAC), AAC priority (offer AAC), and SBC priority (leave both optional offers disabled). SBC remains the service's baseline fallback. The phone chooses the negotiated codec. The receiver screen reports the live raw codec and frequency; LDAC bitrate and frequency are controlled on the phone. Changing the preference while receiver mode is active stops the current sink session, applies both capability flags, and opens connect-wait once. When receiver mode is off, the preference is saved for the next entry and local playback continues. Selecting the current choice emits no action.

Local analysis of Sony's version 1.02 receiver library (SHA-256 `3219b69d9f368ffe5aaa84a26e7d6c7cd377f1fceb45109c9e6a6eee53932d79`) confirmed the exported `SetAAC` and `SetLDAC` symbols and their vtable relocations. The object vptr slots are 20 and 21 respectively; the ELF vtable listing shows 22 and 23 before subtracting its two header words (offset-to-top and RTTI). Disassembly confirms each call's `const bool&` argument ABI. These calls are guarded independently; a normal false return does not skip the next step, while an IPC fault keeps the shared kill switch in effect. Receiver codec values are decoded separately from transmitter preferences: 1 is SBC, 3 AAC, and 4 LDAC; unknown values remain hexadecimal. These values are from the locally verified stock codec map.

No aptX receiver capability was found. The code does not call `SetLDACBufferControl` or expose receiver bitrate/sample-rate controls; those parameters are selected on the phone. No Sony private library or binary is included in this change.

The standalone receiver build was flashed to an NW-A55 and received a basic user trial with a Xiaomi 15. During playback, the device log reported LDAC at 96 kHz and AAC at 44.1 kHz. SBC was observed connected at 44.1 kHz, but audible playback was not established for that case. This was a limited trial; individual preference combinations and long-duration stability were not verified. Source review found that the flashed code silently ignored receiver-switch taps when the main Bluetooth radio was off; the source now shows the existing localized “Bluetooth is off. Turn it on first.” toast for that case. A later diagnostic run confirmed that this toast path ran while Bluetooth was off, though the user has not confirmed seeing it. After Bluetooth was turned on, the user reports that the receiver switch was immediately usable. A logged tap immediately changed the receiver UI state; the native receiver path entered shortly afterward, enabled LDAC, and opened connect-wait. The cause of the earlier report that the switch temporarily failed while Bluetooth was on remains unconfirmed; this later run does not establish that the earlier behavior was fixed.

### Use and regression

1. Turn on the main Bluetooth radio in **Bluetooth** settings, then open **BT Receiver** and choose one of the four receiver codec preferences. The screen explains that the phone chooses the final codec. If Bluetooth is off, the receiver switch displays a localized prompt to turn it on first; it does not power on the radio itself.
2. Turn receiver mode on and pair or connect from the phone. The live status shows the negotiated codec and frequency when available.
3. Change the preference while connected to request one renegotiation. When the receiver switch is off, changing it only saves the preference.
4. The C++ harness includes ten `rx-*` scenarios for offer order, rejected setters, re-entry, absent service, guarded faults, each explicit preference, and active/inactive preference changes. The fake service has a fixed codec response and verifies sequencing; it does not prove phone negotiation.
5. The FFI regression exercises load/get/save and Auto fallback in an isolated process. On ARM test hosts, set `CINDER_FFI_TEST_RUNNER` to the qemu-user executable.

## 中文

Cinder 可调用 Sony `BtPlayerService`，设置手机协商接收编码时可用的能力。接收偏好独立于蓝牙发射设置，并以 `rx_codec` 保存到现有设置文件。旧设置文件、格式错误的值以及 0–3 以外的值都会回退到 Auto。

四种选择分别是：Auto（提供 AAC 和 LDAC）、优先 LDAC（只提供 LDAC）、优先 AAC（只提供 AAC）和优先 SBC（关闭两项可选编码能力）。SBC 仍是服务的基础回退编码，最终协商结果由手机决定。接收页面显示服务报告的实时编码和频率；LDAC 码率和频率由手机设置。接收模式开启时更改偏好，会停止当前接收会话、设置两项能力，然后只重新开启一次等待连接。接收模式关闭时只保存偏好，继续本机播放。选择当前已选项不会触发操作。

本地对 Sony 1.02 接收库（SHA-256 `3219b69d9f368ffe5aaa84a26e7d6c7cd377f1fceb45109c9e6a6eee53932d79`）的分析确认了 `SetAAC`、`SetLDAC` 导出符号和 vtable 重定位。对象 vptr 槽位分别为 20、21；ELF vtable 表中对应 22、23，扣除 offset-to-top 与 RTTI 两个表头项后得到对象槽位。反汇编确认参数 ABI 为 `const bool&`。两次调用分别受保护；普通 `false` 返回不会跳过下一步，IPC 故障则继续遵守共享的 IPC 熔断标记。接收 codec 编号独立于发射侧：1 为 SBC、3 为 AAC、4 为 LDAC；未知值保留十六进制显示。这些值依据本地核实的 stock codec map。

未发现 aptX 接收能力。代码不调用 `SetLDACBufferControl`，也不提供接收端码率或采样率控件；这些参数由手机设置。本改动不包含 Sony 私有库或二进制文件。

独立接收版本已刷入 NW-A55，并由用户使用 Xiaomi 15 做过基本试用。播放期间设备日志报告 LDAC 96 kHz、AAC 44.1 kHz。SBC 曾以 44.1 kHz 显示已连接，但没有确认该情形下有可听播放。这只是有限试用；未逐一验证各偏好组合，也未验证长时间稳定性。源码审查发现，原刷机代码在总蓝牙关闭时会静默忽略接收开关点击；源码现已为此情况显示已有本地化提示“蓝牙已关闭，请先开启。”。后续诊断运行确认总蓝牙关闭时走到这条提示路径，但用户尚未确认看见提示。总蓝牙开启后，用户反馈接收开关立即可用。日志记录一次点击立即将接收 UI 状态切为开启；native 接收流程随后进入、启用 LDAC 并打开等待连接。此前“总蓝牙已开启时开关仍曾暂时无法开启”的根因尚未确认；后续这次成功不能证明首次现象已修复。

### 使用与回归

1. 先在 **Bluetooth** 设置中打开总蓝牙，再进入 **BT Receiver** 并选择四种接收编码偏好之一。页面说明最终编码由手机选择。若蓝牙关闭，接收开关会提示先开启蓝牙，不会自行打开总蓝牙。
2. 开启接收模式后，在手机上配对或连接。服务提供实时状态时，页面会显示协商编码和频率。
3. 已连接时更改偏好会请求一次重新协商；关闭接收开关时更改偏好只会保存。
4. C++ harness 保留十个 `rx-*` 场景，覆盖能力设置顺序、setter 拒绝、重新进入、服务缺失、受保护调用故障、各显式偏好以及接收开启/关闭时更改偏好。fake service 返回固定 codec，只验证调用顺序，不证明手机实际协商结果。
5. FFI 回归在隔离进程中检查设置读取、getter、写回和 Auto 回退。ARM 测试环境通过 `CINDER_FFI_TEST_RUNNER` 指定 qemu-user 可执行文件。
