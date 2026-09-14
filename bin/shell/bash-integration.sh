# Cue command lifecycle. Preserve existing prompt commands and DEBUG handlers.
__cue_running=
__cue_ready=
__cue_prompt() {
  local status=$?
  if [[ -n "$__cue_running" ]]; then
    printf '\033]133;D;%s\007' "$status"
  fi
  __cue_running=
  __cue_ready=
  return "$status"
}
__cue_arm() { __cue_ready=1; }
__cue_preexec() {
  case "$BASH_COMMAND" in
    __cue_prompt|__cue_arm|__cue_ready=*|__cue_running=*) return ;;
  esac
  if [[ -n "$__cue_ready" && -z "$__cue_running" ]]; then
    printf '\033]133;C\007'
    __cue_running=1
  fi
}
# trap -p emits shell-quoted text; decode only the existing shell's own handler.
__cue_trap=$(trap -p DEBUG)
__cue_user_debug=
if [[ -n "$__cue_trap" ]]; then
  __cue_trap=${__cue_trap#trap -- }
  __cue_trap=${__cue_trap% DEBUG}
  eval "__cue_user_debug=$__cue_trap"
fi
# Run existing DEBUG code at top level so its shell context is retained.
trap '__cue_preexec; eval "$__cue_user_debug"' DEBUG
if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
  PROMPT_COMMAND=(__cue_prompt "${PROMPT_COMMAND[@]}" __cue_arm)
else
  PROMPT_COMMAND='__cue_prompt'${PROMPT_COMMAND:+$'\n'"$PROMPT_COMMAND"}$'\n''__cue_arm'
fi
