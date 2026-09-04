# PLAN — getting the SoC into deep idle without wedging the device

**Date:** 2026-09-04 · **Status:** plan only. Nothing here has been executed.
**Prerequisite: the user is holding the device.** This plan exists because the last attempt was run
unattended and cost a forced reboot.

## What we are trying to achieve

`dpidle_cnt` has been **0 for every boot ever sampled**. The SoC has never entered deep idle under
Cinder. The goal of the first test is not power saving — it is a single number: **`dpidle_cnt > 0`,
once, with the device coming back afterwards.**

## What is already known (established, not assumed)

| fact | source |
|---|---|
| Deep idle is gated on the early-suspend flag; `by_vtg` is that flag, not a voltage check | `dpidle_handler` @0xc0037858 |
| The early-suspend chain is only started by a write to `/sys/power/state` | `state_store` is the sole caller of `request_suspend_state` |
| The chain does not stop there — it ends in `pm_autosleep_set_state(3)`, real suspend-to-RAM | dmesg, 2026-09-04 |
| On the cable USB holds a wakeup source, so suspend never completes | why 4 on-cable tests were clean and proved nothing |
| With the gate open, the last clock blocking deep idle is `MT_CG_PERI_USB0` — the cable | `dpidle_block_mask[CG_PERI0]=0x400` |
| `KP` is **not** an armed SPM wake source; `EINT` **is** | `spm_sleep_wakesrc = 0x01204564`, read from the kernel image |
| The power key reaches SPM via MT6323 → **EINT 150**, and its PMIC interrupt is enabled | `PMIC_EINT_SETTING` @0xc0443d74; `INT_CON0 = 0x0420` |
| This unit has **never** completed a suspend/resume | `icx_pm_helper/resume_count = 0` |
| `slp_pwake_time` arms an SPM hardware wake timer, writable from userspace | `spm_get_wake_period` → `spm_set_wakeup_event` sets `PCM_TIMER_EN` |

**So deep idle is only reachable off-cable**, which is also the only state in which the device
cannot be rescued over adb. That tension is the whole problem, and it is why this is a plan rather
than a command.

## The one thing still unknown, and it is the dangerous one

Why the device did not come back on 2026-09-04. A wake path existed on paper. The failure is
therefore most likely in the **resume** path, and nothing measured so far says anything about it.

**Do not treat the wake-path evidence as permission.** Trusting an unexecuted code path is exactly
what caused the wedge. The plan below assumes the resume path is broken until a test shows
otherwise.

## Safety design

Three independent layers, in order of preference:

1. **`slp_pwake_time = 30`** — SPM counts down in hardware with the CPU off and wakes the SoC
   regardless of buttons, cable or userspace. This is the escape that depends on less than what it
   rescues.
   **Caveat, unresolved:** `spm_set_wakeup_event` writes `~wakesrc` to `SPM+0x810`
   (`SLEEP_WAKEUP_EVENT_MASK`) and `PCM_TIMER` is not in the shipped mask. Whether the timer wakes
   through R12 or drives the PCM script directly is **not established**. Treat layer 1 as likely,
   not certain — which is why there are layers 2 and 3.
2. **The power key** — armed on paper (EINT 150), unproven in practice.
3. **The user, holding the device, able to force a reboot.** This is the only layer that has
   actually been demonstrated to work.

**Nothing is wired into cinder-home.** Every step is a shell command run by hand. There is no build
in which Cinder suspends itself, and there must not be until this is settled.

## Procedure

### Phase 0 — on the cable, prove the instrument (no risk)

```sh
cinder-probe --pm                      # baseline: resume_count MUST read 0
cat /sys/module/mt_sleep/parameters/slp_pwake_time    # expect -1
```

### Phase 1 — on the cable, arm the timer and confirm it is accepted

```sh
echo 30 > /sys/module/mt_sleep/parameters/slp_pwake_time
cat    /sys/module/mt_sleep/parameters/slp_pwake_time   # must read 30
```

Nothing suspends yet — on the cable USB holds a wakeup source. This only proves the write takes.

### Phase 2 — the first off-cable test. **User holds the device.**

Screen on, nothing playing. Then, in one shell before unplugging:

```sh
( sleep 20; echo mem > /sys/power/state ) &
```

Unplug. Wait **two minutes** — comfortably longer than the 30 s timer.

**Success:** the screen comes back on its own, or the power key brings it back.
**Failure:** nothing after two minutes → hold power to force a reboot. That is an accepted,
planned outcome, not an emergency.

### Phase 3 — read the post-mortem

Replug, then:

```sh
cinder-probe --pm
```

- `resume_count = 1` → **the device suspended and resumed.** This is the whole objective.
- `spm_r12` names what woke it: `PCM_TIMER` = the timer worked; `EINT` = the power key path worked.
- `suspend_ts` / `resume_ts` give how long it was down.
- `resume_count = 0` after a forced reboot → it never resumed; stop, and treat the resume path as
  the next RE target rather than retrying.

### Phase 4 — only if Phase 3 succeeded: measure the actual prize

Re-arm, unplug, leave it 10 minutes, replug and read:

```sh
cat /sys/kernel/debug/cpuidle/dpidle_state     # dpidle_cnt — the number this is all for
cat /proc/clkmgr/clk_test | grep -i usb        # USB0 should now be OFF
```

`dpidle_cnt > 0` is the result. **The counters are zeroed by a reboot**, so read them before
anything else — that mistake already cost one dataset.

## Abort conditions

Stop and do not retry the same way if any of these hold:

- Phase 1's write does not read back.
- Phase 2 fails twice. Two forced reboots is enough; the resume path is then the problem and it
  should be reverse-engineered offline from `icx_pm_helper_resume` and the late-resume handler list,
  not probed by repeatedly wedging the device.
- Anything about the device's behaviour differs from this plan in a way not written down here.

## What must not happen

- **No wiring into cinder-home.** Not on screen-off, not on idle, not behind a setting.
- **Never arm a suspend and then remove the cable** unless a wake has already been demonstrated —
  the cable is the only thing that can write `on`.
- **Do not run this unattended.** The device must be in someone's hand.

## Related

- `analysis/RE_kernel_power.md` — the wake-source tables and `icx_pm_helper`
- `analysis/RE_early_suspend.md` — the chain, the wedge, and the rule it broke
- `cinder-home/src/main.cpp` — carries a do-not-rebuild note where the helper was
