# gvm-rs shell shim.
#
# This file is meant to be *sourced* by your shell (bash, zsh), never
# executed. It defines the `gvm` shell function that wraps the gvm-rs
# binary and applies environment changes (GOROOT, GOPATH, PATH) to the
# current shell — the same contract as the original moovweb/gvm.

export GVM_ROOT="${GVM_ROOT:-$HOME/.gvm}"

_gvm_rs_bin() {
    if [ -x "$GVM_ROOT/bin/gvm-rs" ]; then
        printf '%s' "$GVM_ROOT/bin/gvm-rs"
    elif command -v gvm-rs >/dev/null 2>&1; then
        command -v gvm-rs
    else
        echo "gvm: gvm-rs binary not found. Install it, then run: gvm-rs init" >&2
        return 127
    fi
}

gvm() {
    local bin sub
    bin=$(_gvm_rs_bin) || return $?
    sub="${1:-}"
    case "$sub" in
        use)
            # `use` prints shell code on stdout (eval'd) and messages on stderr.
            shift
            local out
            out=$("$bin" use "$@") || return $?
            eval "$out"
            ;;
        pkgset|ps)
            local action="${2:-}"
            case "$action" in
                use|create)
                    # These print shell code on stdout; eval it so GOPATH updates.
                    local out
                    out=$("$bin" "$@") || return $?
                    eval "$out"
                    ;;
                *)
                    "$bin" "$@"
                    ;;
            esac
            ;;
        *)
            "$bin" "$@"
            ;;
    esac
}

# Silently activate the default version (if one is set and installed) when the
# shim is sourced, so a fresh shell starts on the default Go version.
if [ -f "$GVM_ROOT/config/default" ]; then
    _gvm_rs_activate_default() {
        local bin out
        bin=$(_gvm_rs_bin) || return 0
        out=$("$bin" use 2>/dev/null) || return 0
        eval "$out"
    }
    _gvm_rs_activate_default
    unset -f _gvm_rs_activate_default
fi
