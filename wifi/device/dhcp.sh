#!/system/bin/sh
# udhcpc event script: apply the lease to the interface, record the DNS servers.
# shellcheck disable=SC2154  # udhcpc sets interface, ip, subnet, router and dns
case "$1" in
  bound|renew)
    busybox ifconfig "$interface" "$ip" netmask "${subnet:-255.255.255.0}" up
    if [ -n "$router" ]; then
      while busybox route del default dev "$interface" 2>/dev/null; do :; done
      for r in $router; do busybox route add default gw "$r" dev "$interface"; break; done
    fi
    echo "$dns" > "${WIFI_RUN:-/tmp/cinder-wifi}/dns"
    # Sony ships /etc/resolv.conf -> /tmp/resolv.conf. World-readable: cinder-home (uid 100) runs
    # curl itself and must be able to read the DNS servers.
    for d in $dns; do echo "nameserver $d"; done > /tmp/resolv.conf.new
    chmod 644 /tmp/resolv.conf.new && mv /tmp/resolv.conf.new /tmp/resolv.conf
    echo "lease: ip=$ip router=$router dns=$dns"
    ;;
  deconfig) busybox ifconfig "$interface" 0.0.0.0 ;;
esac
