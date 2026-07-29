# ldac-bridge — on-device test procedure

Two things can only be confirmed on hardware:

1. **Does the control plane open the socket?** Does `SetLdac(true)` + `SetCurrentSource(true)`
   make `BtTransmitterService` bind/listen on the abstract socket whose name `GetSocketName()`
   returns, so our `connect()` succeeds?
2. **Can we capture the USB-DAC PCM?** Does `snd_pcm_open(…, CAPTURE)` succeed on the UAC card, or
   does the stock `UsbDeviceAudioPlayerService` hold it and give us `-EBUSY`?

**Both are answered by `cinder-probe --ldac`.** No `.UPG` flash, no reboot, no `/contents` trigger
files — one `adb push` and one command. The standalone `cinder-ldac-bridge` daemon is *not* the
bring-up vehicle; §5 explains why.

## 0. Safety

- A verified **wbrt eMMC backup** must already exist (Part E0). Non-negotiable.
- The probe does the easel/appmgr lifecycle **not at all**, so it cannot register as the Home app
  and cannot affect boot. The running UI is untouched. Nothing here needs to be undone.

## 1. Push the probe

```bash
adb push cinder-home/dist/dev/cinder-probe /tmp/cinder-probe
adb shell 'chmod 755 /tmp/cinder-probe'
```

`/tmp` is the only writable *exec* mount — `/data` and `/contents` are `noexec`.

## 2. Get USB-DAC **and** BT-LDAC up at the same time

Stock blocks this in the UI (`disconnectMsgOverlay` + `RequestDisconnection`). Use the reverse
order to sidestep the gate:

1. Plug into the PC and enter **USB-DAC mode first** (no BT connected yet → no disconnect prompt).
   Start audio from the PC so the UAC path is live.
2. **Then** connect the **LDAC headphones** via the BT settings.

If step 2 is refused even with USB-DAC already active, that itself is a finding — the block is
broader than the entry overlay. Note it and stop; we revisit policy.

## 3. Run it

```bash
adb shell 'LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib \
           /tmp/cinder-probe --ldac'
```

Every call is watchdog-bounded, so a **hang** prints its own PC + backtrace instead of just
stopping. Q2 is asked whatever Q1 answers — one run classifies both.

## 4. Read the result

**First, check the pump.** Every line ends with a `pump ticks` count. If it is `0`, the framework
never came up and **nothing else in the log means anything** — the probe says so explicitly. That
check exists because a dead pump does not produce clean failures, it produces plausible-looking
garbage (see §5).

| Log shows | Meaning | Next step |
|---|---|---|
| `Q1 PASS` + `Q2 PASS` | Control plane and capture both good. | Build the pump loop: read PCM → write the socket. The remaining risk is format/chunk negotiation only. |
| `Q1 PASS` + `Q2 FAIL — … is BUSY` | Control plane works; **capture contention** confirmed (stock UAC owns the card). | This is a contention problem, not an RE problem: stop/redirect `UsbDeviceAudioPlayerService`, or replace `libaudiohal-uacalsasingletrack.so` to tee PCM to our socket. |
| `Q1 INCONCLUSIVE — the call threw` | `GetSocketName` **throws** when no source is open. Measured 2026-07-29 with no headphones connected: the first three calls (`SetLdac`, `SetLdacSoundQuality`, `SetCurrentSource`) all returned cleanly, then this one threw. Not evidence against the control plane. | Redo §2 properly and re-run. This is the expected result without a link. |
| `Q1 FAIL — GetSocketName returned EMPTY` **with a link up and a non-zero pump tick count** | The control-plane assumption really is wrong. | Ghidra: re-check the open trigger in `FUN_00019aa0`'s callers — the open may need a different method/arg, or `NotifyOpenAudio` on the *service* side rather than the client. |
| `Q2 INCONCLUSIVE — no capture PCM at all` | The UAC gadget isn't up. | Re-do §2: `sys.sony.config` must be `uac` **and** the PC must actually be feeding audio. The UAC card only exists while both are true. |
| `pump never ticked` | The framework didn't start. | Nothing else in the run is valid. Check `StartForApplication`'s return in the log. |

Also useful while it runs: `adb shell 'cat /proc/asound/card*/pcm*/sub*/status'` to see which
substreams are RUNNING, and `adb logcat` — `hagodaemon` logs every service entry/exit with
`file:line`, so silence there means the request never left our process.

### What a dry run already established (2026-07-29, no headphones, no USB-DAC)

Worth knowing before the real run, because it narrows what is left to find out:

- The framework comes up and the pump is turning **before** the first client call (`3339 ticks`), so
  nothing below it is stack garbage — the trap in §5 is ruled out for this path.
- `BtTransmitterServiceClientFactory::CreateInstance()` returns a real object.
- `SetLdac(true)`, `SetLdacSoundQuality(Auto)` and `SetCurrentSource(true)` all return without
  throwing or hanging. The RE'd vtable indices are therefore at least plausible.
- `GetSocketName()` **throws** with no link up. So the socket genuinely does not exist until a
  source is open, and an empty name only means something once §2 is really in place.

So the only untested step is the one that needs the hardware: does a live LDAC link make
`GetSocketName` return a name we can `connect()` to.

## 5. Why not run the `cinder-ldac-bridge` daemon for this?

Because it cannot answer either question, and it fails in the most misleading way available.

`libBtTransmitterService` is a `pst::services::*` client, so its calls are **asynchronous**: the
request goes over binder and the reply is delivered by `pst::core::Framework`'s event looper.
Nothing dispatches that looper unless someone drives `Framework::Pump()`, and the daemon starts no
framework at all. Sony's wrappers **do not initialise their out-params before the IPC**, so with no
pump a call does not fail cleanly — it returns whatever was on the stack. On PlayerService that
same trap produced `Connect()` "returning" `0xb6xxxxxx`, `IsConnected()` reading garbage and
reporting `true`, and a `SetTrackSequence` "error 99" that was never an error code. It cost weeks.

Here it would surface as `GetSocketName returned empty` — which is exactly row 3 of the table
above, and would send the next session off to redo Ghidra work on a control plane that was fine.

`cinder-probe` starts the framework itself (`Framework::GetReference()` →
`StartForApplication(job, true)` → a `Pump()` thread) — the pattern already proven by `--pump`,
which is how the "playback does nothing" bug was actually found. It also brings the crash handler,
the per-call watchdog and the backtrace dumper, which is what a first bring-up wants.

The daemon keeps its place as the *eventual* shape, and it now prints the warning above at startup
so a stray run can't be misread. But once Q1/Q2 are answered, the pipeline most likely moves inside
**cinder-home** rather than staying a separate process: cinder-home is an easel app, so it has a
live framework already, and the queue/UI state the feature needs is there too.

## 6. Installing the daemon (only once the answers are in)

```
tools/flash.sh --push <downloads>/cinder-ldac-bridge   # stage the binary to device root
tools/flash.sh <downloads>/ldac_install.upg            # installs binary + supervisor
```

Reboot. The supervisor (`ldac-run.sh`) is launched at boot by the Cinder launcher and idles until
`/contents/ldac_on` appears; it runs even with Cinder disabled, so it can be exercised under the
stock UI. Stop with `rm /contents/ldac_on`, disable with `touch /contents/ldac_off`, remove
entirely with `ldac_uninstall.upg`.
