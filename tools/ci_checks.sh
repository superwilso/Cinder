#!/usr/bin/env bash
# ci_checks.sh — the ci.yml checks, here, in one go, before handing work over.
#
#   tools/ci_checks.sh            everything below, keeps going past a failure, summary at the end
#   tools/ci_checks.sh quick      skip the slow ones (release tests, the two harness boots)
#
# Mirrors .github/workflows/ci.yml on Linux: locales, cargo fmt / clippy / test for player,
# installer and rescue, the host syntax check, reuse check, shell selftests, shell lint, payload
# attributes, and the harness boot with the stub and the real UI. Not the payload build (that is
# tools/ci_local.sh) and not the Windows/macOS legs.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
QUICK=0; [ "${1:-}" = quick ] && QUICK=1
fails=()
step() {
    local name="$1"; shift
    printf '── %s\n' "$name"
    if ! "$@" >/tmp/ci_checks_step.log 2>&1; then
        tail -n 25 /tmp/ci_checks_step.log
        fails+=("$name")
    fi
}
step "locales" python3 tools/generate_locales.py --check
for d in player installer rescue; do
    step "$d: fmt" bash -c "cd $d && cargo fmt --all -- --check"
    step "$d: clippy" bash -c "cd $d && cargo clippy -q --all-targets -- -D warnings"
    [ "$QUICK" = 1 ] || step "$d: test" bash -c "cd $d && cargo test -q --release"
done
step "host syntax" tools/host_syntax_check.sh
step "app5 stubs" python3 tools/gen_app5_stubs.py --check
step "ffi header" bash -c "python3 tools/check_ffi_header.py --self-test && python3 tools/check_ffi_header.py"
step "reuse" bash -c "python3 tools/reuse_check.py --self-test && python3 tools/reuse_check.py"
step "selftests" bash cinder-home/tools/selftests.sh
command -v shellcheck >/dev/null && step "shell lint" tools/shell_check.sh
step "payload attrs" tools/check_payload_attrs.sh
if [ "$QUICK" = 0 ]; then
    step "harness (stub UI)" cinder-home/harness/run.sh
    step "harness (real UI)" env CINDER_HARNESS_REAL_FFI=1 cinder-home/harness/run.sh
fi
if [ ${#fails[@]} -eq 0 ]; then echo "ci_checks: all passed"; else
    printf 'ci_checks: FAILED — %s\n' "${fails[@]}"; exit 1; fi
