let enabled = false;

export function developerProbesEnabled() {
  return import.meta.env.DEV && enabled;
}

export function setDeveloperProbesEnabled(value: boolean) {
  enabled = import.meta.env.DEV && value;
}
