# TopCard command lifecycle. Preserve existing prompt commands and DEBUG handlers.
__topcard_running=
__topcard_ready=
__topcard_prompt() {
  local status=$?
  if [[ -n "$__topcard_running" ]]; then
    printf '\033]133;D;%s\007' "$status"
  fi
  __topcard_running=
  __topcard_ready=
  return "$status"
}
__topcard_arm() { __topcard_ready=1; }
__topcard_preexec() {
  case "$BASH_COMMAND" in
    __topcard_prompt|__topcard_arm|__topcard_ready=*|__topcard_running=*) return ;;
  esac
  if [[ -n "$__topcard_ready" && -z "$__topcard_running" ]]; then
    printf '\033]133;C\007'
    __topcard_running=1
  fi
}
# trap -p emits shell-quoted text; decode only the existing shell's own handler.
__topcard_trap=$(trap -p DEBUG)
__topcard_user_debug=
if [[ -n "$__topcard_trap" ]]; then
  __topcard_trap=${__topcard_trap#trap -- }
  __topcard_trap=${__topcard_trap% DEBUG}
  eval "__topcard_user_debug=$__topcard_trap"
fi
# Run existing DEBUG code at top level so its shell context is retained.
trap '__topcard_preexec; eval "$__topcard_user_debug"' DEBUG
if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
  PROMPT_COMMAND=(__topcard_prompt "${PROMPT_COMMAND[@]}" __topcard_arm)
else
  PROMPT_COMMAND='__topcard_prompt'${PROMPT_COMMAND:+$'\n'"$PROMPT_COMMAND"}$'\n''__topcard_arm'
fi
