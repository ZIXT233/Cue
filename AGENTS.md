# Agent Instructions

## Verification Preference

Do not run the app, builds, tests, or graphical UI verification unless the user explicitly requests it. The user runs and compiles the project themselves; agent builds can block their work. Limit verification to source review.

## Non-Interactive Shell Commands

Always use non-interactive flags with file operations to avoid hanging on confirmation prompts.

```bash
cp -f source dest
mv -f source dest
rm -f file
rm -rf directory
```
