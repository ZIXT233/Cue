# Que command lifecycle. Preserve existing prompt commands and DEBUG handlers.
__que_running=
__que_ready=
__que_prompt() {
  local status=$?
  if [[ -n "$__que_running" ]]; then
    printf '\033]133;D;%s\007' "$status"
  fi
  __que_running=
  __que_ready=
  return "$status"
}
__que_arm() { __que_ready=1; }
__que_preexec() {
  case "$BASH_COMMAND" in
    __que_prompt|__que_arm|__que_ready=*|__que_running=*) return ;;
  esac
  if [[ -n "$__que_ready" && -z "$__que_running" ]]; then
    printf '\033]133;C\007'
    __que_running=1
  fi
}
# trap -p emits shell-quoted text; decode only the existing shell's own handler.
__que_trap=$(trap -p DEBUG)
__que_user_debug=
if [[ -n "$__que_trap" ]]; then
  __que_trap=${__que_trap#trap -- }
  __que_trap=${__que_trap% DEBUG}
  eval "__que_user_debug=$__que_trap"
fi
# Run existing DEBUG code at top level so its shell context is retained.
trap '__que_preexec; eval "$__que_user_debug"' DEBUG
if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
  PROMPT_COMMAND=(__que_prompt "${PROMPT_COMMAND[@]}" __que_arm)
else
  PROMPT_COMMAND='__que_prompt'${PROMPT_COMMAND:+$'\n'"$PROMPT_COMMAND"}$'\n''__que_arm'
fi
