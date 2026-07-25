# ldac-bridge — on-device test procedure

The bridge **builds** and the control plane is wired with the real RE'd vtable
indices. Two things can only be confirmed on hardware, and this procedure confirms
them with diagnostics rather than guesswork:

1. **Does the control plane open the socket?** i.e. does `SetLdac(true)` +
   `SetCurrentSource(true)` make `BtTransmitterService` bind/listen on the abstract
   socket whose name `GetSocketName()` returns, so our `connect()` succeeds.
2. **Can we capture the USB-DAC PCM?** i.e. does `snd_pcm_open("hw:4,0", CAPTURE)`
   succeed, or does the stock UAC service (`UsbDeviceAudioPlayerService`) hold
   `card4/pcm0c` and give us `-EBUSY`.

Everything is controlled by files on the storage root (no shell needed).

## 0. Safety
- A verified **wbrt eMMC backup** must already exist (Part E0). Non-negotiable.
- Everything here is reversible over USB-MSC (`rm` a file) or by flashing
  `ldac_uninstall.upg`. No boot-image / init.rc changes.

## 1. Install (once)
```
tools/flash.sh --push <downloads>/cinder-ldac-bridge   # stage the binary to device root
tools/flash.sh <downloads>/ldac_install.upg            # installs binary + supervisor
```
Reboot. The supervisor (`ldac-run.sh`) is launched at boot by the Cinder wrapper and
idles until `/contents/ldac_on` appears. (It runs even with `/contents/cinder_off`
set, so you can test under **stock** UI — recommended for the first run, fewest
variables.)

## 2. Get USB-DAC **and** BT-LDAC up at the same time
Stock blocks this in the UI (`disconnectMsgOverlay` + `RequestDisconnection`). Use
the reverse order to sidestep the gate:
1. Plug into the PC and enter **USB-DAC mode first** (no BT connected yet → no
   disconnect prompt). Start audio from the PC so the UAC path is live.
2. **Then** connect the **LDAC headphones** via the BT settings. Goal: both the
   USB-DAC capture path and the LDAC link are active simultaneously.

If step 2 is refused even with USB-DAC already active, that itself is a finding (the
block is broader than the entry-overlay) — note it and stop; we revisit policy.

## 3. Run the bridge
```
# create an empty file at the storage root:
/contents/ldac_on
```
The supervisor runs `cinder-ldac-bridge`, appending to `/contents/ldac.log`.
Listen on the LDAC headphones for the PC audio. Remove `/contents/ldac_on` to stop.

## 4. Read the result — `tools/flash.sh --cat ldac.log`
Three outcomes, and what each means:

| Log shows | Meaning | Next step |
|---|---|---|
| `socket='@...'` then `bridging USB-DAC -> LDAC`, **and you hear audio** | **Feature works.** Control plane + capture + transmit all good. | Wire it into Cinder's USB-DAC screen; set LDAC quality UI. |
| `socket='@...'` ok, but `snd_pcm_open(hw:4,0): Device or resource busy` | Control plane works; **capture contention** confirmed (stock UAC owns card4). | Implement the contention fix: stop/redirect `UsbDeviceAudioPlayerService`, or replace `libaudiohal-uacalsasingletrack.so` to tee PCM to our socket. |
| `GetSocketName returned empty` / `connect(@...) ... after retries` | Control plane assumption wrong — `SetCurrentSource` did **not** trigger the server socket. | Ghidra: re-check the open trigger in `FUN_00019aa0`'s callers; the open may need a different method/arg (or `NotifyOpenAudio` on the *service* side, not the client). |

Also useful: `--cat ldac_install.log` (install result), and during the run, on a
shell if available, `cat /proc/asound/card*/pcm*/sub*/status` to see which substreams
are RUNNING.

## 5. Cleanup
```
# stop:        rm /contents/ldac_on
# disable:     touch /contents/ldac_off   (supervisor exits)
# full remove: tools/flash.sh <downloads>/ldac_uninstall.upg
```
