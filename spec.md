GPU Dashboard Spec v3

Create a command line tool that displays real-time information about the user's NVIDIA GPU. Update the display once per second.
Metrics to display:

Memory used / total
Utilization %
Temperature
Fan speed
Power draw
Clock speed
Streaming multiprocessor (SM) utilization

Visual style:

Retro terminal aesthetic with ASCII art and colors
Bar graphs for utilization and memory
A thermometer visualization for temperature
A fill-based visual representation for fan speed (not animated)
An ASCII grid representing the GPU's streaming multiprocessors, where each SM lights up (highlighted) when in use, giving a visual map of parallel activity

Technical requirements:

Implemented in Rust
Use NVML (via nvml-wrapper or similar bindings) to query GPU data
Exit when user types q and presses Enter

Error handling:

If no NVIDIA GPU is detected, display a clear error message and exit gracefully

Design consultation required:

Before making significant implementation choices (e.g., TUI library selection, layout structure, how to represent SM data if granular data isn't available), pause and ask the user for input.

Goal: Provide a pleasant, playful experience that satiates the user's curiosity about GPU activity.
