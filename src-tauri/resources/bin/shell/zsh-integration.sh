# Adapted from stablyai/orca b35791365427f097c134d7d856f31053f8aa399a (MIT). See ORCA-LICENSE.
__orca_osc133_precmd() {
  local exit_code=$?
  if [[ -n "${__orca_in_command:-}" ]]; then
    builtin printf "\033]133;D;%s\007" "$exit_code"
    builtin unset __orca_in_command
  fi
  builtin printf "\033]133;A\007"
}
__orca_osc133_preexec() {
  builtin printf "\033]133;C\007"
  # Why typeset -g: a plain assignment here creates a global inside a function,
  # which prints a warning above every command under warn_create_global.
  builtin typeset -g __orca_in_command=1
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __orca_osc133_precmd
add-zsh-hook preexec __orca_osc133_preexec
