#!/bin/sh
# POSIX install prompts for vtladm (source from install.sh; do not execute directly).

vtladm_is_interactive() {
  [ "${VTL_NON_INTERACTIVE:-0}" != "1" ] && [ -t 0 ]
}

vtladm_detect_primary_ip() {
  _ip=""
  if command -v ip >/dev/null 2>&1; then
    _ip=$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i=1;i<=NF;i++) if ($i=="src") {print $(i+1); exit}}')
  fi
  if [ -z "$_ip" ] && command -v hostname >/dev/null 2>&1; then
    _ip=$(hostname -I 2>/dev/null | awk '{print $1}')
  fi
  if [ -n "$_ip" ] && [ "$_ip" != "127.0.0.1" ]; then
    printf '%s\n' "$_ip"
  fi
}

vtladm_validate_ipv4() {
  _v="$1"
  case "$_v" in
    *.*.*.*) ;;
    *) return 1 ;;
  esac
  _o1=${_v%%.*}; _rest=${_v#*.}
  _o2=${_rest%%.*}; _rest=${_rest#*.}
  _o3=${_rest%%.*}; _o4=${_rest#*.}
  for _o in "$_o1" "$_o2" "$_o3" "$_o4"; do
    case "$_o" in
      ''|*[!0-9]*) return 1 ;;
    esac
    if [ "$_o" -lt 0 ] 2>/dev/null || [ "$_o" -gt 255 ] 2>/dev/null; then
      return 1
    fi
  done
}

vtladm_prompt_ipv4() {
  # vtladm_prompt_ipv4 VAR_NAME "prompt" [default]
  _var="$1"
  _prompt="$2"
  _default="${3:-}"
  _val=""

  if [ -z "$_default" ]; then
    _default=$(vtladm_detect_primary_ip 2>/dev/null || true)
  fi
  [ -z "$_default" ] && _default="0.0.0.0"

  if ! vtladm_is_interactive; then
    eval "$_var=\$_default"
    return 0
  fi

  while :; do
    printf '%s [%s]: ' "$_prompt" "$_default"
    IFS= read -r _val || _val=""
    _val=${_val:-$_default}
    if vtladm_validate_ipv4 "$_val"; then
      eval "$_var=\$_val"
      return 0
    fi
    echo "Invalid IPv4 address: $_val" >&2
  done
}

vtladm_set_vtl_conf_kv() {
  _conf="$1"
  _key="$2"
  _value="$3"
  [ -f "$_conf" ] || return 1
  if grep -qE "^[[:space:]]*${_key}[[:space:]]*=" "$_conf" 2>/dev/null; then
    _tmp=$(mktemp "${TMPDIR:-/tmp}/vtl.conf.XXXXXX") || return 1
    sed -E "s|^[[:space:]]*${_key}[[:space:]]*=.*|${_key}=${_value}|" "$_conf" >"$_tmp"
    mv "$_tmp" "$_conf"
  else
    printf '\n%s=%s\n' "$_key" "$_value" >>"$_conf"
  fi
}
