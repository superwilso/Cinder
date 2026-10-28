cinder-extras: for flashing by hand and for recovery. Most people need none of this:
the installer for your computer has both packages inside it and flashes them itself.

  cinder-home-install.upg              install or update Cinder (stock Sony firmware)
  cinder-home-uninstall.upg            remove Cinder (stock Sony firmware)
  cinder-home-install.nw-wm1a.upg      the same two, for a player running Walkman One,
  cinder-home-uninstall.nw-wm1a.upg    which checks packages with a different key

  A package with the wrong key is refused silently: the player restarts and nothing changes.

  LICENSE-libusb.txt   the licence of libusb (GNU LGPL 2.1), which the macOS installer and
                       every cinder-rescue contain. Its source and how to relink: the release notes.

Recovery steps: RECOVERY.md and UNINSTALL.md in the repository.
