# Project: GPU Dashboard

## Overview
A real-time terminal dashboard for NVIDIA GPU monitoring, built in Rust.

## Working Style
- Before making significant design decisions, pause and ask me for input. Examples:
  - Choice of TUI library
  - Layout or visual structure
  - How to handle missing or unavailable data
  - Any tradeoff between simplicity and functionality
- For small implementation details (variable names, function structure, error message wording), use your judgment.
- Explain your reasoning when you propose options.

## Technical Constraints
- Language: Rust
- GPU interface: NVML (nvml-wrapper crate or similar)
- Must compile on Linux with NVIDIA drivers installed

## Quality Bar
- Code should compile without warnings
- Handle errors gracefully (no panics on missing GPU)
- Comments where logic is non-obvious
