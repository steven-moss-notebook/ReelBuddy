# ReelBuddy 🦀🏖️

> An AI desktop companion and tactile video sifter that combs through your footage to curate, trim, and assemble short-form reel treasures.

ReelBuddy lives on your desktop as an interactive, transparent companion widget powered by [Bevy](https://bevyengine.org/). Drop in raw video footage, and your beachcomber buddy sifts through frames using local multimodal AI intelligence—identifying hooks, scoring energy, recommending framing, and polishing clips into export-ready reels.

---

## ✨ Features

- **🏝️ Ambient Desktop Companion**: Frameless, transparent, always-on-top floating pet that rests in the corner of your workspace. Drag it anywhere or feed it videos on demand.
- **🧠 100% Local AI Vision**: Powered by Gemma 4 via `llama-cpp-2` and `hf-hub`. All video frame analysis, hook curation, and text generation run entirely offline on your local hardware.
- **⚡ Hardware Accelerated**: Built-in Metal acceleration on macOS, with optional CUDA and Vulkan feature flags for Linux and Windows.
- **🌊 Tactile Sifting & Sand Simulation**:
  - Gabor phase-field analysis to evaluate frame texture and dynamic visual energy.
  - Interactive sand canvas and GPU particle streams using `bevy_hanabi`.
  - Museum table progression where excavated video clips are assembled and cataloged.
- **🎬 Automated Reel Assembly**:
  - Video decoding and sampling via `video-rs` and `ffmpeg`.
  - Dynamic aspect-ratio reframing, intelligent crop selection, hook generation, and caption burning.
  - Interactive chore queue: Hunt candidate segments $\rightarrow$ Cut & trim $\rightarrow$ Hook duel $\rightarrow$ Remix $\rightarrow$ Ship to `saved_reels/`.
- **🤖 Headless Bot Interface (`--stdio`)**: JSON-lines IPC protocol over stdin/stdout for scripting, automation, and CI pipelines without launching the GUI.
- **💾 Local SQLite Persistence**: Stores all sources, window segments, generated clips, curated chores, and companion stats in an embedded SQLite database (`rusqlite`).

---

## 📋 Prerequisites

- **Rust**: Latest stable Rust toolchain (2024 edition supported).
- **FFmpeg**: `video-rs` requires FFmpeg development libraries installed on your system:
  - **macOS**: `brew install ffmpeg pkg-config`
  - **Ubuntu / Debian**: `sudo apt update && sudo apt install -y libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev pkg-config`
  - **Arch Linux**: `sudo pacman -S ffmpeg pkgconf`
- **C/C++ Compiler & CMake**: Required by `llama-cpp-2` to compile native kernels.

---

## 🚀 Getting Started

### 1. Clone the Repository

```bash
git clone https://github.com/steven-moss-notebook/ReelBuddy.git
cd ReelBuddy
```

### 2. Build and Run

#### macOS (Metal Acceleration)
Metal acceleration is enabled automatically on macOS:

```bash
cargo run --release
```

#### Linux / Windows with NVIDIA CUDA
Enable the `cuda` feature flag:

```bash
cargo run --release --features cuda
```

#### Vulkan
Enable the `vulkan` feature flag:

```bash
cargo run --release --features vulkan
```

---

## 🎮 Controls & Shortcuts

| Action | Control |
|---|---|
| **Feed Video** | Drag & drop an `.mp4` or `.mov` file directly onto the companion window |
| **Open File Dialog** | Press <kbd>O</kbd> |
| **Move Window** | Click and drag the companion frame |
| **Quit** | Press <kbd>Esc</kbd> |

### Command-Line Arguments

- `--debug`: Launch with debug diagnostics and expanded editor desk controls.
- `--stdio`: Run in headless JSON-lines bot mode over standard input/output.

```bash
# Debug mode
cargo run --release -- --debug

# Headless stdio mode
cargo run --release -- --stdio
```

---

## 🏗️ Architecture

```
src/
├── main.rs            # Application entry point, Bevy app setup and system chains
├── window.rs          # Transparent, frameless window management and OS input
├── chores_ui.rs       # Interactive UI cards, buttons, and chore resolution
├── db.rs              # SQLite schema, video sources, window candidates, and pet state
├── field.rs           # Gabor phase-field analysis and signature computation
├── pet_state.rs       # Companion stats, state machine, dialogue, and sampling configs
├── score.rs           # Clip scoring and heuristic evaluations
├── stdio.rs           # Headless line-based JSON IPC driver
├── taste.rs           # Recommendation heuristics and curator taste models
├── media/             # Video decoding, frame sampling, and rendering helpers
├── editor/            # Background engine thread, LLM inference, and chore orchestrator
│   ├── brain.rs       # Local LLM integration (llama-cpp-2)
│   ├── downloader.rs  # Automatic HuggingFace model cache & download
│   ├── actions.rs     # Action parsing, framing, and caption definitions
│   ├── session.rs     # Video editing workspace desk and chore queue
│   └── prompt.rs      # System prompts and structured generation
└── sorter/            # Museum conservation table, sand canvas, and particle effects
    ├── sand_canvas.rs # Sand grid simulation
    ├── sand_hanabi.rs # GPU particle emitter pipelines
    ├── museum.rs      # Excavation curation and specimen display
    └── treasure.rs    # Unearthed clip gleams and collectible rewards
```

---

## 📄 License

This project is dual-licensed under either:

- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in ReelBuddy by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.
