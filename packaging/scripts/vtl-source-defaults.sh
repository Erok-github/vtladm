# Read vtl.conf key=value pairs; export legacy env vars for backwards compatibility.
# Usage: . /opt/vtladm/scripts/vtl-source-defaults.sh && _vtl_source_defaults

_vtl_conf_path() {
  if [ -n "${VTL_CONF_PATH:-}" ]; then
    printf '%s' "$VTL_CONF_PATH"
  elif [ -n "${VTL_PREFIX:-}" ]; then
    printf '%s' "${VTL_PREFIX}/var/vtl.conf"
  else
    printf '%s' "/opt/vtladm/var/vtl.conf"
  fi
}

_vtl_conf_get() {
  _key="$1"
  _f="$(_vtl_conf_path)"
  [ -f "$_f" ] || return 1
  _val=$(grep -E "^[[:space:]]*${_key}[[:space:]]*=" "$_f" 2>/dev/null | tail -1 \
    | sed 's/^[^=]*=//;s/^[[:space:]]*//;s/[[:space:]]*$//;s/^"//;s/"$//' | tr -d '\r') || return 1
  [ -n "$_val" ] && printf '%s' "$_val" && return 0
  return 1
}

_vtl_source_defaults() {
  # Legacy: map vtl.conf keys to old /etc/default/vtladm env vars for remaining consumers.
  # Env vars already set take precedence (runtime overrides).
  : "${VTL_KO:=$(_vtl_conf_get vtl_ko || printf '%s' "/opt/vtladm/ko/vtl.ko")}"
  : "${VTL_TAPE_DIR:=$(_vtl_conf_get tape_dir || printf '%s' "/opt/vtladm/var/tapes")}"
  : "${VTL_INSTANCES:=$(_vtl_conf_get vtl_instances || true)}"
  : "${VTL_INSMOD_EXTRA:=$(_vtl_conf_get insmod_extra || true)}"
  : "${VTLADM_WEB_HOST:=$(_vtl_conf_get web_host || printf '%s' "127.0.0.1")}"
  : "${VTLADM_WEB_PORT:=$(_vtl_conf_get web_port || printf '%s' "8765")}"
  : "${VTLADM_WEB_COOKIE_SECURE:=$(_vtl_conf_get web_cookie_secure || true)}"
  : "${VTL_PATROL_STRICT:=$(_vtl_conf_get patrol_strict || true)}"
  export VTL_KO VTL_TAPE_DIR VTL_INSTANCES VTL_INSMOD_EXTRA
  export VTLADM_WEB_HOST VTLADM_WEB_PORT VTLADM_WEB_COOKIE_SECURE
  export VTL_PATROL_STRICT
}
