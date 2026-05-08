<p align="center">
  <img src="ui/assets/icon.svg" alt="S" width="12%" align="middle" /><img src="ui/assets/title.svg" alt="AUCEBOTTLE" width="50%" align="middle" />
</p>

<div align="center">
  <i>An anime artwork sorter daemon written in <b><a href="https://github.com/tauri-apps/tauri" style="cursor: pointer">Tauri</a></b> & <b><a href="https://github.com/rust-lang/rust" style="cursor: pointer">Rust</a></b>.</i>
</div>
<br>

<p align="center">
  <img src="https://placehold.co/600x400/1e1e1e/FFF?text=Demo+GIF+1+Placeholder" width="400px" alt="Demo 1 Placeholder" />
  <img src="https://placehold.co/600x400/1e1e1e/FFF?text=Demo+GIF+2+Placeholder" width="400px" alt="Demo 2 Placeholder" />
</p>

<p align="center">
  <img src="https://img.shields.io/github/v/release/GitThirteen/saucebottle?color=orange&style=for-the-badge" alt="Latest Version" />
  <img src="https://img.shields.io/badge/Tauri-FFC131?logo=tauri&logoColor=black&style=for-the-badge" alt="Tauri" />
  <img src="https://img.shields.io/github/actions/workflow/status/GitThirteen/saucebottle/release.yml?style=for-the-badge" alt="Builds" />
  <img src="https://img.shields.io/badge/Rust-E05D44?logo=rust&style=for-the-badge" alt="Rust" />
  <img src="https://img.shields.io/badge/Sauce-Extra_Spicy-FF4B2B?style=for-the-badge" alt="Sauce Level" />
</p>

## Introduction

Born from being tired of having to sort downloaded artwork manually, _SauceBottle_ is a lightweight background daemon taking care of the tedious manual work. By monitoring a specified `input` folder, _SauceBottle_ automatically identifies artwork via reverse-image search using IQDB, and organizes your collection into a clean, customizable directory structure.

## How do I use this?

### Features

_SauceBottle_ features include, but are not limited to:

- A fully automatic identification of japanese-styled artwork using IQDB
- Support for multiple booru-boards like [Danbooru](https://danbooru.donmai.us/), [yande.re](https://yande.re/), and [Gelbooru](https://gelbooru.com/)
- Sorting into a customizable folder structure
- Batch downloading all images in a page range given a custom list of tags
- Running in the background / system tray as daemon at essentially 0 cost

### Installation

To download the [latest version](https://github.com/GitThirteen/saucebottle/releases/latest) of _SauceBottle_, please refer to the [Releases](https://github.com/GitThirteen/saucebottle/releases) and follow the instructions in the changelog.

> [!NOTE]
> While _SauceBottle_ supports auto-updating directly through the app, it is still recommended to check the [GitHub page](https://github.com/GitThirteen/saucebottle) every once in a while in case any breaking changes require a full reinstall.

### Usage

> [!IMPORTANT]
> IQDB was primarily written with Danbooru in mind, which in turn means that the vast majority of matches will occur through Danbooru. While _SauceBottle_ will use yande.re if no other credentials have been added, the matching rate is not particularly good. Therefore, please consider adding at least your Danbooru credentials before starting to actively use the application.

#### Custom Folder Structure

Use the **Settings** tab to define your folder hierarchy. You can drag and drop the individual building blocks around into your preferred folder structure (e.g., `Fandom -> Character -> Artist`).

#### Automatic Sorting

To automatically sort your images, you have two ways forward:

1. Drag and drop files or entire folders directly into the application window.
2. Drop files into the `input` folder. The folder is located in your OS `Pictures` directory. (On windows, that would be `Pictures/SauceBottle/input`).

_SauceBottle_ will automatically sweep the folder, identify the sauce (and optionally rename the file), and move it to its new destination in the `results` folder (or your own specified folder).

#### Batch Downloader

If you want to use the batch downloader, navigate to the `Downloader` tab, enter your desired tags, and input a page range to pull images directly from the Booru board of your choice.

#### Daemonization

The daemon will continue running when closed. You can always close it by right-clicking it in your system's tray. As long as it is running (and automatic sweeping is enabled), it will continue to watch your `input` folder in the background with effectively 0 CPU impact.


## How do I build this?

_SauceBottle_ uses [Tauri](https://github.com/tauri-apps/tauri) with a [Vue 3](https://github.com/vuejs/core) / [TypeScript](https://github.com/microsoft/TypeScript) frontend and a [Rust](https://github.com/rust-lang/rust) backend.

### Prerequisites

<img src="https://cdn.simpleicons.org/nodedotjs/339933" alt="Node.js" height="16" /> **Node.js** (v20.0.0 or higher)<br>
<img src="https://cdn.simpleicons.org/rust/text-primary" alt="Rust" height="16" /> **Rust** (v1.70 or higher)

Because _SauceBottle_ uses Tauri to bridge the frontend with a native Rust backend, your operating system must have specific build tools installed before `cargo` can compile the application. The most common OS dependencies are:

**Windows:** Install the [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and ensure the [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) is installed.<br>
**macOS:** Install the Xcode Command Line Tools by running the following in your terminal:
```bash
xcode-select --install
```
**Linux (Debian/Ubuntu):** Install the required webkit and build packages:
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```
> [!NOTE]
> Please refer to the [Tauri v2 prerequisites guide](https://v2.tauri.app/start/prerequisites/) for further information and how to build Tauri if you are using a different operating system.

### Setup

To set up _SauceBottle_ for local development, clone this repository and install the frontend dependencies.

```bash
git clone https://github.com/GitThirteen/saucebottle.git
cd saucebottle
npm install
```

Once the dependencies are installed, you can launch the application in development mode. This will start the Vite frontend server and simultaneously compile and launch the Rust backend:
```bash
npm run tauri dev
```

### Configuration

_SauceBottle_ does not require an `.env` file or complex local environment configuration.

Because the application handles sensitive user data (like Booru API keys and usernames), it utilizes the native OS credential manager (via the `keyring-core` crate) to securely store and retrieve credentials.

To configure the application locally:
1. Launch the app in development mode (`npm run tauri dev`).
2. Navigate to the **Credentials** tab within the application UI.
3. Add your Booru API keys there. They will be securely saved and automatically used in API requests.

#### Building for Release

To compile a highly optimized, production-ready `.msi`, `.app`, or `.deb` installer, run:

```bash
npm run tauri build
```

As Vite can obscure certain issues (especially in regard to Tauri permissions), it is suggested to also test the actual native application in a debug build. The `--debug` flag enables the use of the inspector. Please also refer to the [Tauri v2 debug section](https://v2.tauri.app/develop/debug/).

### Contributing

As this is a project I can only work on in my free time, **any** contribution is massively appreciated!

To contribute, please at first take a look at the **[SauceBottle To-Do board](https://github.com/users/GitThirteen/projects/1)** and / or the [project issues](https://github.com/GitThirteen/saucebottle/issues) (they should be synchronized) to check the status of current tasks, bugfixes, and improvements. If you have a feature suggestion or noticed a bug that has not been tracked yet, **please create an issue for it**.

If you want to contribute but have no starting point, you are free to pick up any task marked as `📝 To Do` on the board. Please let us know in the respective issue that you have started working on it or if you require any additional information.

#### Some minor rules:

- **Code Formatting:** Please ensure your code is formatted before submitting a PR.
  - **For Rust ("backend"):** Run `cargo fmt` and `cargo clippy`.
  - **For Frontend:** Run `npm run lint` (or your preferred formatter).
- **Commit History:** It is totally fine to have multiple commits! Your PRs will be squashed before merging.
- **Versioning:** Do ***not*** bump versions yourself. Unless your PR fixes a critical bug, this will happen automatically when enough features have been accumulated or enough time has passed to warrant a new release.
- **Link Your Issues:** When submitting a Pull Request, please include `Closes #issue_number` (or similar) in the description.

## License

_SauceBottle_ is released under the **[GNU GPLv3 license](https://www.gnu.org/licenses/gpl-3.0.html)**. Please refer to [LICENSE](./LICENSE) for the full license text.