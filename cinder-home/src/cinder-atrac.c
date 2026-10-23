/* cinder-atrac.c — ATRAC playback on any region (the `atrac` component).
 *
 * Sony builds the ATRAC decoder into every A50 (libPlayerService.so: `container_demuxer.oma`,
 * `audio_decoder.at3`) and switches it off outside Japan. The switch is one call:
 * PlayerServiceFactory::CreateInstance asks DmpFeature::IsFeaturedAtrac() once, when the service
 * starts, and logs "ATRAC enabled." or "ATRAC disabled." (PlayerServiceService.cc:149). No other
 * library on the player calls it (nm -D --undefined-only over vendor/sony, 2026-10-22).
 *
 * This library answers true. It is loaded into that one process — hagoromo9, started as
 * `hagodaemon MediaStoreService PlayerService` — by Cinder's hagodaemon wrapper, which preloads
 * HAGO_DIR/<first service>.so (src/cinder-hagowrap.c). Being preloaded, its definition comes before
 * libDmpFeature.so's when libPlayerService.so's PLT entry is resolved.
 *
 * A member function with no arguments: `this` arrives in r0 and is not needed; bool comes back in
 * r0. No libc, no state, nothing that can fail. Removing the file (or the wrapper's off switches)
 * gives Sony's answer back at the next start of the service.
 */
int _ZN3pst10dmpfeature10DmpFeature15IsFeaturedAtracEv(void* self)
{
    (void)self;
    return 1;
}
