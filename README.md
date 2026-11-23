# Fluora

A desktop app for reviewing microscopy images: browse every image in a project, mark the good
ones, write notes, take sharp zoomed-in **snaps** with a scale bar, arrange images on a **board**,
and gather the very best in a **Bouquet**. Made for the slide-scanner and confocal images of the
Elya lab, and works for any set of TIFFs (and CZIs, with Fiji).

**Fluora Renamer**, a second small app in this repository, renames image files to the lab's
naming convention (with undo), on its own.

Both run on **Mac** and **Windows**. Everything you do stays on your own computer.

## Download

Go to **[Releases](../../releases)** and download, for the newest version:

| | Fluora | Fluora Renamer |
|---|---|---|
| **Mac** | `Fluora_x.y.z_universal.dmg` | `Fluora.Renamer_x.y.z_universal.dmg` |
| **Windows** | `Fluora_x.y.z_x64-setup.exe` | `Fluora.Renamer_x.y.z_x64-setup.exe` |

**Mac:** open the `.dmg` and drag the app into Applications. The app isn't signed with an Apple
developer certificate, so the first time, **right-click the app → Open → Open**. If macOS says
the app "is damaged", open Terminal and run `xattr -cr /Applications/Fluora.app`, then open it
again.

**Windows:** run the `setup.exe`. If Windows shows "Windows protected your PC", click
**More info → Run anyway**.

You'll also want **[Fiji](https://fiji.sc)** installed: Fluora uses it to open full-resolution
files and to unpack CZI files.

## Getting started

1. **Open Fluora and type your name.** It labels your marks, notes and snaps.
2. **Name your first project**, then choose the folder with your images (on the lab drive,
   connect the VPN first). Fluora looks in every folder inside it.
3. **Unpack CZIs** if it offers to: Fiji saves each scan as TIFFs (`name_series0.tif`, …), the
   same as the lab's Fiji macro. If the folder is read-only (like `Raw_data`), choose a folder of
   your own to save them in.
4. **Label the images.** Names that already follow the lab's convention are read for you; for the
   rest, fill in what you know. Only the image number is required. You can also **rename the files
   on the drive** to the lab's naming here; it can be undone.
5. **Check the channels**: the name and colour of each stain.
6. **Save and make previews.** A short tour shows you around; Help (the **?** at the top right)
   explains everything and lists the keyboard shortcuts.

## What it does

- **Projects**: separate sets of images, each with their own marks, notes, board and snaps.
- **Review**: open an image, mark it good / okay / not good (`G` `O` `X`), write notes. The
  project's flower opens as you go.
- **Look closer**: zoom, one stain at a time, one brightness and contrast setting per stain so
  conditions compare fairly, and open the full-resolution file in Fiji.
- **Snaps** (`S`): exactly the view you're looking at, made from the full-resolution file, with a
  scale bar. Each snap remembers where it came from, also inside the PNG file.
- **Board**: drag images and snaps into groups, e.g. for a figure, and copy each group's list.
- **Gallery**: your good and okay images side by side.
- **Bouquet** (`P` to pick): a showcase of the best images and snaps of the project.
- **Works offline**: previews are kept on your computer, so browsing doesn't need the VPN.

Images can be slide-scanner pyramids (`G[R1]`, `G[R2]`, …), single-size images of any size,
compressed or tiled TIFFs, or one file per stain (`G[DAPI]`, `G[BF]`, …).

## Your data

Image folders are only ever **read**, unless you choose to rename files in the setup. Marks,
notes, snaps, the board and previews are saved on your computer in Fluora's own folder
(Mac: `~/Library/Application Support/org.cnelyalab.fluora`, Windows: `%APPDATA%\org.cnelyalab.fluora`).
Nothing is uploaded anywhere.

## The lab's file naming

```
[Sample]_Cond_SL##_IM##_Stains_G[x]_Obj_[Focus]_Date_Initials.tif
03p_Cad_SL47_IM01_BF-DAPI_G[BF]_20x_HD_20260419_CNH.tif
```

See [scripts/Filename_Convention.md](scripts/Filename_Convention.md). The original Fiji and
Jupyter scripts are in [scripts/](scripts/).

## Repository

| Folder | What's in it |
|---|---|
| `src-tauri/` | Fluora (Rust, Tauri 2) |
| `renamer/` | Fluora Renamer (Rust, Tauri 2) |
| `ui/` | The screens of both apps (HTML, CSS, JavaScript) |
| `crates/naming/` | Shared code: the naming convention, finding images in folders, renaming and undo, CZI unpacking |
| `crates/update/` | Shared code: checking GitHub for a newer version and installing it |
| `scripts/` | The lab's original Fiji macros and Jupyter notebooks |

### Building it yourself

Install [Rust](https://rustup.rs) and the Tauri CLI (`cargo install tauri-cli --version "^2"`), then:

```bash
cd src-tauri && cargo tauri build     # Fluora
cd renamer && cargo tauri build       # Fluora Renamer
```

### Making a new release

On GitHub, open **Actions → Build installers → Run workflow**, type a version such as `0.2.0`, and
click **Run workflow**. About 30 minutes later a draft release with the Mac and Windows installers
of both apps appears under **Releases**; check it and click **Publish release**.

Once it is published, everyone's installed Fluora and Fluora Renamer (version 0.2.0 or later)
notice the next time they open: a card asks "Update now?", and on yes the app downloads the new
version, swaps itself and restarts. Nobody has to download from GitHub again.

**The update-signing key.** Each build is signed so the apps only accept updates that really came
from this repository. The private key lives outside the repository, at
`~/.tauri/fluora-updater.key` on the maintainer's Mac, and in the repository's GitHub secret
`TAURI_SIGNING_PRIVATE_KEY` (Settings → Secrets and variables → Actions). Keep a backup of that
file: without it, already-installed apps can't be updated automatically any more. To build on your
own computer, point the build at the key:
`TAURI_SIGNING_PRIVATE_KEY=~/.tauri/fluora-updater.key TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" cargo tauri build`
