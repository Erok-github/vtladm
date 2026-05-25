# Source /etc/default/vtladm without failing on Windows CRLF (common when copied from git on Windows).
# Usage: . /opt/vtladm/scripts/vtl-source-defaults.sh && _vtl_source_defaults /etc/default/vtladm

_vtl_source_defaults() {
  _f="${1:-/etc/default/vtladm}"
  [ -f "$_f" ] || return 0
  if command -v mktemp >/dev/null 2>&1; then
    _t=$(mktemp) || _t="/tmp/vtladm-defaults.$$"
  else
    _t="/tmp/vtladm-defaults.$$"
  fi
  if ! sed 's/\r$//' "$_f" > "$_t"; then
    rm -f "$_t"
    return 1
  fi
  # shellcheck disable=SC1090
  . "$_t"
  rm -f "$_t"
}
